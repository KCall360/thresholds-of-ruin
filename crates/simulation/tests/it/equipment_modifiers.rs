use tor_simulation::{
    combat::DamageType,
    content_ids::DescriptorId,
    creatures::{CreatureState, SourceId},
    damage::{DamageComponent, DamageSpec},
    dice::Edge,
    rules::{DefenseKind, Modifier, ModifierOp, Scope, Selector, Stat},
    ItemId,
};

fn modifier(op: ModifierOp) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op,
    }
}

fn creature() -> CreatureState {
    CreatureState::new(super::creature_fixture::build(
        super::creature_fixture::species(),
    ))
    .unwrap()
}

#[test]
fn equipment_changes_fold_into_the_existing_cache_and_keep_source_attribution() {
    let mut creature = creature();
    let before = creature.derived().defenses.physical;
    let armor = vec![modifier(ModifierOp::Add {
        stat: Stat::Defense {
            defense: DefenseKind::Physical,
        },
        value: 3,
    })];
    creature
        .set_equipment(vec![(ItemId(19), armor.clone())])
        .unwrap();
    assert_eq!(creature.derived().defenses.physical, before + 3);
    assert_eq!(
        creature
            .derived()
            .sources
            .get(&SourceId::Equipment(ItemId(19))),
        Some(&armor)
    );
    creature.rebuild(creature.build().clone()).unwrap();
    assert_eq!(creature.derived().defenses.physical, before + 3);
    creature.set_equipment(vec![]).unwrap();
    assert_eq!(creature.derived().defenses.physical, before);
    assert!(!creature
        .derived()
        .sources
        .contains_key(&SourceId::Equipment(ItemId(19))));
}

#[test]
fn equipment_fire_immunity_uses_the_same_kernel_for_environmental_damage() {
    let mut creature = creature();
    let fire = DescriptorId::new("fire").unwrap();
    creature
        .set_equipment(vec![(
            ItemId(9),
            vec![modifier(ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: fire.clone(),
                },
            })],
        )])
        .unwrap();
    let hazard = DamageSpec::new(
        vec![DamageComponent::fixed(DamageType::Energy, Some(fire), 17)],
        None,
    )
    .unwrap();
    let result = hazard.resolve(
        &mut 42,
        Edge::default(),
        &creature.derived().protection,
        &mut (),
    );
    assert_eq!((result.raw, result.total), (17, 0));
    creature.set_equipment(vec![]).unwrap();
    assert_eq!(
        hazard
            .resolve(
                &mut 42,
                Edge::default(),
                &creature.derived().protection,
                &mut ()
            )
            .total,
        17
    );
}

#[test]
fn rejected_equipment_is_atomic_and_duplicate_sources_are_rejected() {
    let mut creature = creature();
    let before = creature.clone();
    assert!(creature
        .set_equipment(vec![(
            ItemId(4),
            vec![modifier(ModifierOp::Reduction {
                selector: Selector::Category {
                    category: DamageType::Impact
                },
                amount: u32::MAX,
            })]
        )])
        .is_err());
    assert_eq!(creature, before);
    assert!(creature
        .set_equipment(vec![(ItemId(4), vec![]), (ItemId(4), vec![])])
        .is_err());
    assert_eq!(creature, before);
}

#[test]
fn removing_health_equipment_preserves_injury_and_cannot_revive_a_dead_creature() {
    let mut creature = creature();
    let original = creature.health().maximum();
    creature
        .set_equipment(vec![(
            ItemId(1),
            vec![modifier(ModifierOp::Add {
                stat: Stat::Health,
                value: 10,
            })],
        )])
        .unwrap();
    creature.damage(original + 1);
    assert!(!creature.health().dead());
    let removed = creature.set_equipment(vec![]).unwrap();
    assert!(removed.died);
    assert_eq!(creature.health().injury(), original + 1);
    creature
        .set_equipment(vec![(
            ItemId(1),
            vec![modifier(ModifierOp::Add {
                stat: Stat::Health,
                value: 100,
            })],
        )])
        .unwrap();
    assert!(creature.health().dead());
    assert_eq!(creature.health().current(), 0);
}

#[test]
fn collision_damage_saturates_at_the_shared_injury_bound() {
    assert_eq!(tor_simulation::combat::impact_damage(i64::MAX), 1_000_000);
    assert_eq!(tor_simulation::combat::impact_damage(i64::MIN), 1_000_000);
    assert_eq!(tor_simulation::combat::impact_damage(2048), 0);
    assert_eq!(tor_simulation::combat::impact_damage(8192), 12);
}

#[test]
fn multiple_weapon_sockets_select_the_highest_equipped_slot_once() {
    use tor_simulation::{
        AnatomySpec, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game, ItemClass, ItemSpec,
    };
    let mut species = super::creature_fixture::species();
    species.anatomy = AnatomySpec {
        slots: vec![EquipmentSlot::Weapon, EquipmentSlot::Weapon],
    };
    let mut game = Game::two_room(
        42,
        super::creature_fixture::content_with(species.clone(), vec![]),
    );
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    super::creature_fixture::configure(&mut game, actor, "neutral", species);
    let low = super::creature_fixture::melee(1, 40, DamageType::Keen, 3);
    let high = super::creature_fixture::melee(2, 80, DamageType::Impact, 7);
    for (id, slot, attack) in [(11, 1, high.clone()), (12, 0, low)] {
        let mut item = ItemSpec::ordinary(format!("weapon {id}"));
        item.class = ItemClass::Weapon;
        item.equipment = Some(EquipmentSpec {
            slot: EquipmentSlot::Weapon,
            attack: Some(attack),
            modifiers: vec![],
        });
        game.place_item_stack(id, at, Some(actor), 1, item).unwrap();
        game.equip_authored(actor, ItemId(id), EquipmentSlotId(slot))
            .unwrap();
    }
    assert_eq!(game.selected_melee_attack(actor), Some(&high));
}

#[test]
fn ordinary_equipment_does_not_require_a_combat_creature() {
    use tor_simulation::{
        AnatomySpec, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game, ItemClass, ItemSpec,
    };
    let mut game = Game::two_room(42, super::creature_fixture::content());
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_anatomy(
        actor,
        AnatomySpec {
            slots: vec![EquipmentSlot::BodyArmor],
        },
    )
    .unwrap();
    let mut armor = ItemSpec::ordinary("mail".into());
    armor.class = ItemClass::Armor;
    armor.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::BodyArmor,
        attack: None,
        modifiers: vec![modifier(ModifierOp::Add {
            stat: Stat::Defense {
                defense: DefenseKind::Physical,
            },
            value: 3,
        })],
    });
    game.place_item_stack(10, at, Some(actor), 1, armor)
        .unwrap();
    game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
        .unwrap();
    assert_eq!(
        game.equipment(actor).unwrap().get(&EquipmentSlotId(0)),
        Some(&ItemId(10))
    );
    assert!(game.creature(actor).is_none());
}
