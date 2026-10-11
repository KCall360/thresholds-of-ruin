use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{ai::AiProfile, diagnostics::work_counts, Action, ActorId, Game};
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
    super::creature_fixture::configure(
        &mut game,
        ai,
        "neutral",
        super::creature_fixture::species(),
    );
    assert!(
        game.creature(ai).is_some(),
        "AI integration subjects own their builds"
    );
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
fn autonomous_armor_replacement_resumes_checkpointed_removal_before_equipping_upgrade() {
    use tor_simulation::{
        checkpoint::SharedState, AnatomySpec, EquipmentSlot, EquipmentSlotId, EquipmentSpec,
        ItemClass, ItemId, ItemSpec,
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
    let mut species = super::creature_fixture::species();
    species.anatomy = AnatomySpec {
        slots: vec![EquipmentSlot::BodyArmor],
    };
    super::creature_fixture::configure(&mut game, ai, "neutral", species);
    game.configure_ai(ai, AiProfile::default()).unwrap();
    for (id, defense) in [(10, 1), (11, 3)] {
        let mut item = ItemSpec::ordinary(format!("mail {id}"));
        item.class = ItemClass::Armor;
        item.equipment = Some(EquipmentSpec {
            slot: EquipmentSlot::BodyArmor,
            attack: None,
            defense,
            reductions: BTreeMap::new(),
        });
        game.place_item_stack(id, at(2), Some(ai), 1, item).unwrap();
    }
    game.equip_authored(ai, ItemId(10), EquipmentSlotId(0))
        .unwrap();
    game.refresh_navigation();
    game.act(human, Action::Wait).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((ai, Action::Unequip { item: ItemId(10) }))
    );
    game.admit_ai_intention(ai).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(game.preparation(ai).is_some());
    assert_eq!(game.equipment(ai).unwrap()[&EquipmentSlotId(0)], ItemId(10));
    let mut shared = SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(restored.preparation(ai), game.preparation(ai));
    for _ in 0..16 {
        if restored.preparation(ai).is_none() {
            break;
        }
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    assert!(restored.preparation(ai).is_none());
    assert!(restored.equipment(ai).unwrap().is_empty());
    for _ in 0..4 {
        if restored.next_actor() == Some(ai) {
            break;
        }
        restored.act(human, Action::Wait).unwrap();
    }
    assert_eq!(
        restored.next_ai_action(),
        Some((
            ai,
            Action::Equip {
                item: ItemId(11),
                slot: EquipmentSlotId(0)
            }
        ))
    );
    restored.admit_ai_intention(ai).unwrap();
    restored.execute_next_intention().unwrap().outcome.unwrap();
    assert!(restored.equipment(ai).unwrap().is_empty());
    for _ in 0..16 {
        if restored.preparation(ai).is_none() {
            break;
        }
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    assert!(restored.preparation(ai).is_none());
    assert_eq!(
        restored.equipment(ai).unwrap()[&EquipmentSlotId(0)],
        ItemId(11)
    );
}

#[test]
fn autonomous_loot_checkpoint_preserves_split_stack_and_does_not_collect_spares() {
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
    super::creature_fixture::configure(
        &mut game,
        ai,
        "neutral",
        super::creature_fixture::species(),
    );
    game.configure_ai(ai, AiProfile::default()).unwrap();
    let mut potion = ItemSpec::ordinary("healing".into());
    potion.class = ItemClass::Potion;
    potion.stackable = true;
    potion.consumable = Some(ConsumableSpec {
        effects: vec![EffectSpec::Heal { amount: 10 }],
    });
    game.place_item_stack(20, at(2), None, 3, potion).unwrap();
    game.refresh_navigation();
    game.act(human, Action::Wait).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((
            ai,
            Action::Take {
                item: ItemId(20),
                quantity: Some(1)
            }
        ))
    );
    game.admit_ai_intention(ai).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let mut shared = SharedState::default();
    let snapshot =
        serde_json::from_value(serde_json::to_value(game.checkpoint(&mut shared)).unwrap())
            .unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    let view = restored.observe(ai).unwrap();
    assert_eq!(view.inventory.len(), 1);
    assert_eq!(view.inventory[0].quantity, 1);
    assert_eq!(
        view.ground_items
            .iter()
            .find(|item| item.id == ItemId(20))
            .unwrap()
            .quantity,
        2
    );
    for _ in 0..4 {
        if restored.next_actor() == Some(ai) {
            break;
        }
        restored.act(human, Action::Wait).unwrap();
    }
    let (next, action) = restored
        .next_ai_action()
        .expect("AI must have its next decision");
    assert_eq!(next, ai);
    assert!(!matches!(action, Action::Take { .. }));
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
        super::creature_fixture::configure(
            &mut game,
            ActorId(id),
            if ActorId(id) == ai { "foe" } else { "hero" },
            super::creature_fixture::species(),
        );
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
