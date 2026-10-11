use std::{collections::BTreeMap, num::NonZeroU64};
use tor_simulation::{
    checkpoint::SharedState, combat::DamageType, Action, ActorId, AnatomySpec, ConsumableSpec,
    EffectSpec, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game, IntentionOrigin, ItemClass,
    ItemId, ItemSpec,
};
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(combat: bool) -> (Game, ActorId, ActorId) {
    fixture_with_anatomy(
        combat,
        AnatomySpec {
            slots: vec![
                EquipmentSlot::BodyArmor,
                EquipmentSlot::Ring,
                EquipmentSlot::Ring,
            ],
        },
    )
}
fn fixture_with_anatomy(combat: bool, anatomy: AnatomySpec) -> (Game, ActorId, ActorId) {
    let mut game = Game::two_room(42);
    let first = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let second = game
        .spawn_actor(at(2), NonZeroU64::new(50).unwrap())
        .unwrap();
    if combat {
        let mut species = super::creature_fixture::species();
        species.anatomy = anatomy;
        super::creature_fixture::configure(&mut game, first, "neutral", species);
    } else {
        game.configure_anatomy(first, anatomy).unwrap();
    }
    game.refresh_navigation();
    (game, first, second)
}
fn armor(game: &mut Game, actor: ActorId) {
    let mut spec = ItemSpec::ordinary("mail".into());
    spec.class = ItemClass::Armor;
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::BodyArmor,
        attack: None,
        defense: 3,
        reductions: BTreeMap::new(),
    });
    game.place_item_stack(10, at(1), Some(actor), 1, spec)
        .unwrap();
}
fn finish(game: &mut Game, actor: ActorId) {
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
}

#[test]
fn noncombat_armor_uses_anatomy_and_takes_three_turns_to_complete() {
    let (mut game, actor, observer) = fixture(false);
    armor(&mut game, actor);
    let id = game
        .admit_intention(
            actor,
            Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(0),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(game.equipment(actor).unwrap().is_empty());
    assert_eq!(game.preparation(actor).unwrap().intention, Some(id));
    let view = game.observe(actor).unwrap();
    assert!(view.combat.is_none());
    let interaction = view.interactions.unwrap();
    assert_eq!(interaction.preparation.unwrap().remaining, 300);
    assert_eq!(interaction.inventory[0].equipped_slot, None);
    finish(&mut game, actor);
    assert_eq!(
        game.observe(actor).unwrap().interactions.unwrap().completed,
        vec![tor_simulation::Work::Equip {
            item: ItemId(10),
            slot: EquipmentSlotId(0)
        }]
    );
    assert!(game
        .observe(observer)
        .unwrap()
        .interactions
        .is_none_or(|view| view.completed.is_empty()));
    assert_eq!(game.tick(), 300);
    assert_eq!(
        game.equipment(actor).unwrap()[&EquipmentSlotId(0)],
        ItemId(10)
    );
    assert!(game
        .admit_intention(
            actor,
            Action::Drop {
                item: ItemId(10),
                quantity: None
            },
            IntentionOrigin::Human
        )
        .is_err());
}

#[test]
fn damage_pauses_item_work_and_saved_resume_keeps_original_intention() {
    let (mut game, actor, _) = fixture(true);
    armor(&mut game, actor);
    let id = game
        .admit_intention(
            actor,
            Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(0),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.apply_effects(
        actor,
        &[EffectSpec::Damage {
            components: BTreeMap::from([(DamageType::Vital, 1)]),
        }],
    )
    .unwrap();
    let progress = game.preparation(actor).unwrap().clone();
    assert!(!progress.active);
    assert_eq!(game.effective_combat(actor).unwrap().defense, 10);
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot = serde_json::from_value(serde_json::to_value(snapshot).unwrap()).unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(restored.preparation(actor), Some(&progress));
    assert!(restored.cancel_intention(actor, id).is_err());
    restored.resume_intention(actor, id).unwrap();
    // Let the independently due actor run before the queued continuation.
    while restored.next_intention_actor() != Some(actor) {
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    restored.execute_next_intention().unwrap().outcome.unwrap();
    finish(&mut restored, actor);
    assert_eq!(restored.effective_combat(actor).unwrap().defense, 13);
}

#[test]
fn duplicate_ring_slots_are_distinct_and_a_slot_must_match_the_item() {
    let (mut game, actor, _) = fixture(false);
    let mut spec = ItemSpec::ordinary("ring".into());
    spec.class = ItemClass::Ring;
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::Ring,
        attack: None,
        defense: 0,
        reductions: BTreeMap::new(),
    });
    for id in [11, 12] {
        game.place_item_stack(id, at(1), Some(actor), 1, spec.clone())
            .unwrap();
    }
    game.equip_authored(actor, ItemId(11), EquipmentSlotId(1))
        .unwrap();
    game.equip_authored(actor, ItemId(12), EquipmentSlotId(2))
        .unwrap();
    assert_eq!(game.equipment(actor).unwrap().len(), 2);
    assert!(game
        .equip_authored(actor, ItemId(12), EquipmentSlotId(0))
        .is_err());
}

#[test]
fn drinking_consumes_at_completion_and_identifies_only_observable_effects() {
    let (mut game, actor, _) = fixture(true);
    let mut spec = ItemSpec::ordinary("healing".into());
    spec.class = ItemClass::Potion;
    spec.concealed = true;
    spec.appearance = "red vial".into();
    spec.stackable = true;
    spec.consumable = Some(ConsumableSpec {
        effects: vec![EffectSpec::Heal { amount: 10 }],
    });
    game.place_item_stack(20, at(1), Some(actor), 2, spec)
        .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    assert_eq!(game.observe(actor).unwrap().inventory[0].quantity, 2);
    finish(&mut game, actor);
    let view = game.observe(actor).unwrap();
    assert_eq!(view.inventory[0].quantity, 1);
    assert!(!view.inventory[0].identified);
    game.apply_effects(
        actor,
        &[EffectSpec::Damage {
            components: BTreeMap::from([(DamageType::Vital, 5)]),
        }],
    )
    .unwrap();
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    finish(&mut game, actor);
    assert!(game.observe(actor).unwrap().inventory.is_empty());
    assert_eq!(
        game.observe(actor).unwrap().interactions.unwrap().completed,
        vec![tor_simulation::Work::Drink { item: ItemId(20) }]
    );
    assert_eq!(game.health(actor).unwrap().0, 30);
    let mut replacement = ItemSpec::ordinary("healing".into());
    replacement.class = ItemClass::Potion;
    replacement.concealed = true;
    replacement.appearance = "red vial".into();
    game.place_item_stack(21, at(1), Some(actor), 1, replacement)
        .unwrap();
    assert!(game.observe(actor).unwrap().inventory[0].identified);
}

#[test]
fn consumed_objective_remains_unmet_and_its_checkpoint_is_valid() {
    let (mut game, actor, _) = fixture(true);
    let mut spec = ItemSpec::ordinary("objective potion".into());
    spec.class = ItemClass::Potion;
    spec.consumable = Some(ConsumableSpec {
        effects: vec![EffectSpec::Heal { amount: 1 }],
    });
    game.place_item_stack(20, at(1), Some(actor), 1, spec)
        .unwrap();
    game.configure_run(
        actor,
        std::collections::BTreeSet::from([actor]),
        Some(tor_simulation::combat::Objective {
            anchor: at(4),
            item: Some(ItemId(20)),
            continue_play: false,
            disclosed: true,
        }),
        BTreeMap::new(),
    )
    .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    finish(&mut game, actor);
    assert!(game.observe(actor).unwrap().inventory.is_empty());
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert!(Game::restore_checkpoint(snapshot, &shared).is_some());
    assert!(!game.observe(actor).unwrap().combat.unwrap().victory);
}

#[test]
fn invalid_effect_sequence_is_atomic_and_lethal_sequence_never_revives() {
    let (mut game, actor, _) = fixture(true);
    let before = game.clone();
    assert!(game
        .apply_effects(
            actor,
            &[
                EffectSpec::Damage {
                    components: BTreeMap::from([(DamageType::Vital, 5)])
                },
                EffectSpec::Heal { amount: 0 },
            ]
        )
        .is_err());
    assert_eq!(game, before);
    game.apply_effects(
        actor,
        &[
            EffectSpec::Damage {
                components: BTreeMap::from([(DamageType::Vital, 100)]),
            },
            EffectSpec::Heal { amount: 100 },
        ],
    )
    .unwrap();
    assert_eq!(game.health(actor).unwrap().0, 0);
    assert!(!game.alive(actor));
}

#[test]
fn lethal_final_potion_reports_completion_without_anatomy_or_remaining_inventory() {
    let (mut game, actor, observer) = fixture_with_anatomy(true, AnatomySpec::default());
    let mut potion = ItemSpec::ordinary("poison".into());
    potion.class = ItemClass::Potion;
    potion.concealed = true;
    potion.appearance = "red potion".into();
    potion.consumable = Some(ConsumableSpec {
        effects: vec![EffectSpec::Damage {
            components: BTreeMap::from([(DamageType::Vital, 100)]),
        }],
    });
    game.place_item_stack(20, at(1), Some(actor), 1, potion)
        .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    finish(&mut game, actor);
    assert!(!game.alive(actor));
    let view = game.observe(actor).unwrap();
    assert!(view.inventory.is_empty());
    assert_eq!(
        view.interactions.unwrap().completed,
        vec![tor_simulation::Work::Drink { item: ItemId(20) }]
    );
    assert!(game
        .observe(observer)
        .unwrap()
        .interactions
        .is_none_or(|view| view.completed.is_empty()));
}

#[test]
fn armor_protects_until_removal_finishes_and_death_drops_it_once() {
    let (mut game, actor, witness) = fixture(true);
    armor(&mut game, actor);
    game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
        .unwrap();
    game.act(actor, Action::Unequip { item: ItemId(10) })
        .unwrap();
    assert_eq!(game.effective_combat(actor).unwrap().defense, 13);
    finish(&mut game, actor);
    assert_eq!(game.effective_combat(actor).unwrap().defense, 10);
    game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
        .unwrap();
    game.apply_effects(
        actor,
        &[EffectSpec::Damage {
            components: BTreeMap::from([(DamageType::Vital, 100)]),
        }],
    )
    .unwrap();
    assert!(game.equipment(actor).unwrap().is_empty());
    let view = game.observe(witness).unwrap();
    assert_eq!(
        view.ground_items
            .iter()
            .filter(|item| item.id == ItemId(10))
            .count(),
        1
    );
    assert_eq!(
        view.ground_items
            .iter()
            .filter(|item| item.class == ItemClass::Corpse)
            .count(),
        1
    );
}

#[test]
fn concealed_equipment_discloses_its_slot_but_stats_require_actor_knowledge() {
    let (mut game, actor, witness) = fixture(true);
    let mut spec = ItemSpec::ordinary("enchanted ring".into());
    spec.class = ItemClass::Ring;
    spec.concealed = true;
    spec.appearance = "silver ring".into();
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::Ring,
        attack: None,
        defense: 3,
        reductions: BTreeMap::new(),
    });
    game.place_item_stack(30, at(1), Some(actor), 1, spec)
        .unwrap();
    let interaction = game.observe(actor).unwrap().interactions.unwrap();
    assert_eq!(interaction.inventory[0].slot, Some(EquipmentSlot::Ring));
    assert!(interaction.inventory[0].known_equipment.is_none());
    assert!(game.observe(witness).unwrap().interactions.is_none());
    game.learn_identity(actor, "enchanted ring").unwrap();
    let interaction = game.observe(actor).unwrap().interactions.unwrap();
    assert_eq!(
        interaction.inventory[0]
            .known_equipment
            .as_ref()
            .unwrap()
            .defense,
        3
    );
}

#[test]
fn newly_visible_hostility_pauses_work_but_resume_accepts_the_existing_threat() {
    let (mut game, actor, other) = fixture(true);
    game.configure_run(
        actor,
        std::collections::BTreeSet::from([actor]),
        None,
        BTreeMap::from([(
            "neutral".into(),
            std::collections::BTreeSet::from(["enemy".into()]),
        )]),
    )
    .unwrap();
    armor(&mut game, actor);
    game.act(
        actor,
        Action::Equip {
            item: ItemId(10),
            slot: EquipmentSlotId(0),
        },
    )
    .unwrap();
    super::creature_fixture::configure(
        &mut game,
        other,
        "enemy",
        super::creature_fixture::species(),
    );
    game.act(other, Action::Wait).unwrap();
    assert!(!game.preparation(actor).unwrap().active);
    assert!(game.equipment(actor).unwrap().is_empty());
    game.act(
        actor,
        Action::Equip {
            item: ItemId(10),
            slot: EquipmentSlotId(0),
        },
    )
    .unwrap();
    assert!(game.preparation(actor).unwrap().active);
    finish(&mut game, actor);
    assert_eq!(game.equipment(actor).unwrap().len(), 1);
}

#[test]
fn effective_equipment_changes_attack_and_typed_damage_without_mutating_base_rules() {
    let (mut game, actor, _) = fixture_with_anatomy(
        true,
        AnatomySpec {
            slots: vec![EquipmentSlot::Weapon, EquipmentSlot::BodyArmor],
        },
    );
    let natural_attack = game.effective_combat(actor).unwrap().attack.clone();
    let mut weapon = ItemSpec::ordinary("sword".into());
    weapon.class = ItemClass::Weapon;
    let attack = tor_simulation::attacks::MeleeAttack::fixed(
        tor_simulation::attributes::Skill::HeavyWeaponry,
        100,
        30,
        40,
        DamageType::Keen,
        None,
        10,
    )
    .unwrap();
    weapon.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::Weapon,
        attack: Some(attack.clone()),
        defense: 0,
        reductions: BTreeMap::new(),
    });
    game.place_item_stack(30, at(1), Some(actor), 1, weapon)
        .unwrap();
    game.equip_authored(actor, ItemId(30), EquipmentSlotId(0))
        .unwrap();
    assert_eq!(game.selected_melee_attack(actor), Some(&attack));
    let mut armor = ItemSpec::ordinary("armor".into());
    armor.class = ItemClass::Armor;
    armor.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::BodyArmor,
        attack: None,
        defense: 3,
        reductions: BTreeMap::from([(DamageType::Impact, 2)]),
    });
    game.place_item_stack(31, at(1), Some(actor), 1, armor)
        .unwrap();
    game.equip_authored(actor, ItemId(31), EquipmentSlotId(1))
        .unwrap();
    game.apply_effects(
        actor,
        &[EffectSpec::Damage {
            components: BTreeMap::from([(DamageType::Impact, 5)]),
        }],
    )
    .unwrap();
    assert_eq!(game.health(actor).unwrap().0, 27);
    let mut build = game.creature(actor).unwrap().build().clone();
    let mut template = tor_simulation::creatures::Template::new("vital_immunity", 0);
    template
        .grants
        .push(tor_simulation::grants::Grant::Immunity(
            tor_simulation::grants::Selector::Category(DamageType::Vital),
        ));
    build.set_templates(vec![template]).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
    assert!(!game
        .apply_effects(
            actor,
            &[EffectSpec::Damage {
                components: BTreeMap::from([(DamageType::Vital, 5)])
            }]
        )
        .unwrap());
    assert_eq!(game.health(actor).unwrap().0, 27);
    game.act(actor, Action::Unequip { item: ItemId(30) })
        .unwrap();
    finish(&mut game, actor);
    assert_eq!(game.effective_combat(actor).unwrap().attack, natural_attack);
    assert_eq!(game.effective_combat(actor).unwrap().defense, 13);
}
