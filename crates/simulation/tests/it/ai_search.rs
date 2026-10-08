use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    ai::AiProfile, combat::CombatSpec, diagnostics::work_counts, Action, ActorId, Game,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

#[test]
fn autonomous_healing_intention_survives_checkpoint_and_consumes_only_on_completion() {
    use tor_simulation::{
        checkpoint::SharedState, ConsumableSpec, EffectSpec, ItemClass, ItemId, ItemSpec,
    };
    let mut game = Game::two_room(42);
    let at = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    };
    let human = game
        .spawn_actor(at(1), NonZeroU64::new(50).unwrap())
        .unwrap();
    let ai = game
        .spawn_actor(at(2), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_combat(ai, CombatSpec::default()).unwrap();
    game.configure_ai(ai, AiProfile::default()).unwrap();
    let mut potion = ItemSpec::ordinary("healing".into());
    potion.class = ItemClass::Potion;
    potion.stackable = true;
    potion.concealed = true;
    potion.appearance = "red potion".into();
    potion.consumable = Some(ConsumableSpec {
        effects: vec![EffectSpec::Heal { amount: 10 }],
    });
    game.place_item_stack(20, at(2), Some(ai), 2, potion)
        .unwrap();
    game.identify_item(ai, ItemId(20)).unwrap();
    game.apply_effects(
        ai,
        &[EffectSpec::Damage {
            components: BTreeMap::from([(tor_simulation::combat::DamageType::Vital, 15)]),
        }],
    )
    .unwrap();
    game.refresh_navigation();
    game.act(human, Action::Wait).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((ai, Action::Drink { item: ItemId(20) }))
    );
    game.admit_ai_intention(ai).unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.action, Some(Action::Drink { item: ItemId(20) }));
    execution.outcome.unwrap();
    assert_eq!(game.observe(ai).unwrap().inventory[0].quantity, 2);
    let mut shared = SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(restored.preparation(ai), game.preparation(ai));
    while restored.preparation(ai).is_some() {
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    assert_eq!(restored.health(ai).unwrap().0, 25);
    assert_eq!(restored.observe(ai).unwrap().inventory[0].quantity, 1);
}

#[test]
fn a_many_target_decision_uses_one_remembered_topology_search() {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_region(Region {
            id: RegionId(1),
            name: "arena".into(),
            bounds: Extent::new(16, 16, 1).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 42);
    let at = |x, y| Location {
        region: RegionId(1),
        position: Position { x, y, z: 0 },
    };
    let turn = NonZeroU64::new(100).unwrap();
    let human = game.spawn_actor(at(7, 7), turn).unwrap();
    let ai = game.spawn_actor(at(7, 8), turn).unwrap();
    for x in 5..9 {
        for y in 5..9 {
            if (x, y) != (7, 7) && (x, y) != (7, 8) {
                game.spawn_actor(at(x, y), turn).unwrap();
            }
        }
    }
    for id in 1..=16 {
        game.configure_combat(
            ActorId(id),
            CombatSpec {
                faction: if ActorId(id) == ai { "foe" } else { "hero" }.into(),
                ..Default::default()
            },
        )
        .unwrap();
    }
    game.configure_run(
        human,
        BTreeSet::from([human]),
        None,
        BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
    )
    .unwrap();
    game.configure_ai(ai, AiProfile::default()).unwrap();
    game.refresh_navigation();
    game.act(human, Action::Wait).unwrap();
    assert_eq!(game.observe(ai).unwrap().visible_actors.len(), 15);
    let before = work_counts().route_searches;
    assert_eq!(
        game.next_ai_action(),
        Some((ai, Action::Attack { target: human }))
    );
    assert_eq!(work_counts().route_searches - before, 1);
}
