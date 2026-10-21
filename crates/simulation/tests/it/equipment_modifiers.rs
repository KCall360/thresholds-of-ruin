use tor_simulation::{
    combat::DamageType,
    content_ids::DescriptorId,
    creatures::{CreatureState, EquipmentSource, SourceId},
    damage::{DamageComponent, DamageSpec},
    dice::Edge,
    rules::{DefenseKind, Modifier, ModifierOp, Scope, Selector, Stat},
    ItemId,
};

fn equipped(
    creature: &mut CreatureState,
    entries: Vec<(ItemId, Vec<Modifier>)>,
) -> Result<tor_simulation::creatures::RebuildOutcome, tor_simulation::creatures::CreatureStateError>
{
    creature.set_equipment(
        entries
            .into_iter()
            .enumerate()
            .map(|(index, (item, modifiers))| EquipmentSource {
                weapon_profile: None,
                item,
                socket: tor_simulation::EquipmentSlotId(index as u16),
                modifiers,
                attack: None,
            })
            .collect(),
    )
}

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
    equipped(&mut creature, vec![(ItemId(19), armor.clone())]).unwrap();
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
    equipped(&mut creature, vec![]).unwrap();
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
    equipped(
        &mut creature,
        vec![(
            ItemId(9),
            vec![modifier(ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: fire.clone(),
                },
            })],
        )],
    )
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
    equipped(&mut creature, vec![]).unwrap();
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
    assert!(equipped(
        &mut creature,
        vec![(
            ItemId(4),
            vec![modifier(ModifierOp::Reduction {
                selector: Selector::Category {
                    category: DamageType::Impact
                },
                amount: u32::MAX,
            })]
        )]
    )
    .is_err());
    assert_eq!(creature, before);
    assert!(equipped(
        &mut creature,
        vec![(ItemId(4), vec![]), (ItemId(4), vec![])]
    )
    .is_err());
    assert_eq!(creature, before);
}

#[test]
fn removing_health_equipment_preserves_injury_and_cannot_revive_a_dead_creature() {
    let mut creature = creature();
    let original = creature.health().maximum();
    equipped(
        &mut creature,
        vec![(
            ItemId(1),
            vec![modifier(ModifierOp::Add {
                stat: Stat::Health,
                value: 10,
            })],
        )],
    )
    .unwrap();
    creature.damage(u32::try_from(original + 1).unwrap());
    assert!(!creature.health().dead());
    let removed = equipped(&mut creature, vec![]).unwrap();
    assert!(removed.died);
    assert_eq!(creature.health().injury(), original + 1);
    equipped(
        &mut creature,
        vec![(
            ItemId(1),
            vec![modifier(ModifierOp::Add {
                stat: Stat::Health,
                value: 100,
            })],
        )],
    )
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
fn multiple_weapon_sockets_select_only_the_authored_primary() {
    use tor_simulation::{AnatomySpec, EquipmentSlot, EquipmentSlotId, Game, ItemClass, ItemSpec};
    for has_primary in [true, false] {
        let mut species = super::creature_fixture::species();
        species.anatomy = AnatomySpec {
            capacities: tor_simulation::equipment_capacity::EquipmentCapacities::new(
                std::collections::BTreeMap::from([(
                    tor_simulation::content_ids::EquipmentCapacityId::new("grip").unwrap(),
                    2,
                )]),
            )
            .unwrap(),
            primary_attack_socket:
                tor_simulation::equipment_capacity::PrimaryAttackSocket::Socket {
                    slot: tor_simulation::EquipmentSlotId(0),
                },
            slots: vec![
                EquipmentSlot::new("weapon").unwrap(),
                EquipmentSlot::new("weapon").unwrap(),
            ],
        };
        if !has_primary {
            species.anatomy.primary_attack_socket =
                tor_simulation::equipment_capacity::PrimaryAttackSocket::None;
        }
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
        let natural = game.selected_melee_attack(actor).unwrap().clone();
        let low = super::creature_fixture::melee(1, 40, DamageType::Keen, 3);
        let high = super::creature_fixture::melee(2, 80, DamageType::Impact, 7);
        for (id, slot, attack) in [(11, 1, high.clone()), (12, 0, low.clone())] {
            let mut item = ItemSpec::ordinary(format!("weapon {id}"));
            item.class = ItemClass::Weapon;
            item.equipment = Some(tor_simulation::EquipmentSpec {
                periodic_relocation: None,
                potency_modifiers: Vec::new(),
                weapon_profile: None,
                armor: None,
                slot: EquipmentSlot::new("weapon").unwrap(),
                attack: Some(attack),
                modifiers: vec![],
            });
            game.place_item_stack(id, at, Some(actor), 1, item).unwrap();
            game.equip_authored(actor, ItemId(id), EquipmentSlotId(slot))
                .unwrap();
            if slot == 1 {
                assert_eq!(game.selected_melee_attack(actor), Some(&natural));
            }
        }
        let expected = if has_primary {
            tor_simulation::attacks::SelectedMeleeAttack::Weapon(low)
        } else {
            natural
        };
        assert_eq!(game.selected_melee_attack(actor), Some(&expected));
    }
}

#[test]
fn ordinary_equipment_does_not_require_a_combat_creature() {
    use tor_simulation::{AnatomySpec, EquipmentSlot, EquipmentSlotId, Game, ItemClass, ItemSpec};
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
            capacities: Default::default(),
            primary_attack_socket: tor_simulation::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![EquipmentSlot::new("body_armor").unwrap()],
        },
    )
    .unwrap();
    let mut armor = ItemSpec::ordinary("mail".into());
    armor.class = ItemClass::Armor;
    armor.equipment = Some(tor_simulation::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: Some(tor_simulation::ArmorSpec {
            physical_reduction: 3,
            material: tor_simulation::ArmorMaterial::Metal,
            rustproof: false,
        }),
        slot: tor_simulation::EquipmentSlot::new("body_armor").unwrap(),
        attack: None,
        modifiers: vec![],
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

#[test]
fn selected_attack_cache_tracks_weapon_operands_rebuild_removal_and_restore() {
    use tor_simulation::{
        attacks::MeleeAttack,
        attributes::{Attribute, Skill},
        content_ids::AbilityId,
        creatures::record::CreatureRecord,
        EquipmentSlotId,
    };
    let mut creature = creature();
    let natural = creature.derived().selected_attack.clone();
    let mut build = creature.build().clone();
    build
        .train(tor_simulation::HdIndex(0), Skill::LightWeaponry)
        .unwrap();
    creature.rebuild(build).unwrap();
    let attack = MeleeAttack::fixed(
        Skill::LightWeaponry,
        100,
        90,
        70,
        DamageType::Energy,
        Some(DescriptorId::new("fire").unwrap()),
        7,
    )
    .unwrap();
    let equipment = vec![EquipmentSource {
        weapon_profile: None,
        item: ItemId(100),
        socket: EquipmentSlotId(0),
        modifiers: vec![],
        attack: Some(attack.clone()),
    }];
    creature.set_equipment(equipment.clone()).unwrap();
    let selected = &creature.derived().selected_attack;
    assert_eq!(selected.source, SourceId::Equipment(ItemId(100)));
    assert_eq!(selected.socket, Some(EquipmentSlotId(0)));
    assert_eq!(
        selected.attack,
        tor_simulation::attacks::SelectedMeleeAttack::Weapon(attack)
    );
    assert_eq!(
        (
            selected.attribute,
            selected.attribute_value,
            selected.rank,
            selected.check_bonus
        ),
        (Attribute::Speed, 0, 1, 101)
    );
    let melee = AbilityId::new("basic_melee").unwrap();
    assert_eq!(
        creature.derived().ability_timings[&melee],
        tor_simulation::creatures::AbilityTiming {
            preparation: 90,
            recovery: 70
        }
    );
    creature.rebuild(creature.build().clone()).unwrap();
    assert_eq!(creature.derived().selected_attack.check_bonus, 101);
    let record = CreatureRecord::capture(&creature);
    let encoded = serde_json::to_value(&record).unwrap();
    for cache in [
        "equipment",
        "selected_attack",
        "health_contributions",
        "ability_timings",
        "ability_reaches",
    ] {
        assert!(
            encoded.get(cache).is_none(),
            "{cache} must never be persisted"
        );
    }
    let restored = record
        .restore(creature.build().content().clone(), equipment)
        .unwrap();
    assert_eq!(restored, creature);
    creature.set_equipment(vec![]).unwrap();
    assert_eq!(creature.derived().selected_attack, natural);
}

#[test]
fn duplicate_equipment_sockets_are_rejected_without_changing_cached_provenance() {
    use tor_simulation::EquipmentSlotId;
    let mut creature = creature();
    let before = creature.clone();
    let equipment = [ItemId(4), ItemId(5)]
        .into_iter()
        .map(|item| EquipmentSource {
            weapon_profile: None,
            item,
            socket: EquipmentSlotId(0),
            modifiers: vec![],
            attack: None,
        })
        .collect();
    assert!(creature.set_equipment(equipment).is_err());
    assert_eq!(creature, before);
}

#[test]
fn out_of_range_equipment_sockets_cannot_enter_cached_attack_provenance() {
    use tor_simulation::{creatures::CreatureStateError, EquipmentSlotId};
    let mut creature = creature();
    let before = creature.clone();
    for socket in [64, u16::MAX] {
        let equipment = vec![EquipmentSource {
            weapon_profile: None,
            item: ItemId(100),
            socket: EquipmentSlotId(socket),
            modifiers: vec![],
            attack: Some(
                tor_simulation::attacks::MeleeAttack::new(
                    creature.build().species().melee.skill(),
                    creature.build().species().melee.bonus(),
                    creature.build().species().melee.wind_up(),
                    creature.build().species().melee.recovery(),
                    creature
                        .build()
                        .species()
                        .melee
                        .first_primary_damage()
                        .unwrap(),
                )
                .unwrap(),
            ),
        }];
        assert_eq!(
            creature.set_equipment(equipment),
            Err(CreatureStateError::Inconsistent)
        );
        assert_eq!(creature, before);
    }
}

#[test]
fn selective_speed_equipment_preserves_real_seed_defense_through_set_floor_and_removal() {
    use tor_simulation::attributes::{Attribute, Attributes, ByAttribute};
    let mut build = super::creature_fixture::build(super::creature_fixture::species());
    build
        .set_initial_attributes(
            Attributes::new(ByAttribute {
                speed: 4,
                strength: 3,
                ..ByAttribute::default()
            })
            .unwrap(),
        )
        .unwrap();
    let mut creature = CreatureState::new(build).unwrap();
    let baseline = creature.derived().defenses.physical;
    let speed = creature.derived().attributes.get(Attribute::Speed);
    let selective = |value| {
        modifier(ModifierOp::AttributeWithoutDefense {
            attribute: Attribute::Speed,
            defense: DefenseKind::Physical,
            value,
        })
    };
    equipped(&mut creature, vec![(ItemId(91), vec![selective(2)])]).unwrap();
    assert_eq!(
        creature.derived().attributes.get(Attribute::Speed),
        speed + 2
    );
    assert_eq!(creature.derived().defenses.physical, baseline);
    equipped(&mut creature, vec![(ItemId(91), vec![selective(-100)])]).unwrap();
    assert_eq!(creature.derived().attributes.get(Attribute::Speed), 0);
    assert_eq!(creature.derived().defenses.physical, baseline);
    let set = modifier(ModifierOp::Set {
        stat: Stat::Attribute {
            attribute: Attribute::Speed,
        },
        value: 6,
    });
    equipped(&mut creature, vec![(ItemId(92), vec![set.clone()])]).unwrap();
    let set_defense = creature.derived().defenses.physical;
    equipped(
        &mut creature,
        vec![(ItemId(92), vec![set]), (ItemId(91), vec![selective(2)])],
    )
    .unwrap();
    assert_eq!(creature.derived().attributes.get(Attribute::Speed), 6);
    assert_eq!(creature.derived().defenses.physical, set_defense);
    equipped(&mut creature, vec![]).unwrap();
    assert_eq!(creature.derived().attributes.get(Attribute::Speed), speed);
    assert_eq!(creature.derived().defenses.physical, baseline);
}

#[test]
fn equipment_recovery_is_presence_based_and_removal_restores_period() {
    let mut creature = creature();
    let period = creature.derived().health_recovery_period;
    let rate = modifier(ModifierOp::HealthRecoveryRate { multiplier: 2 });
    equipped(
        &mut creature,
        vec![
            (ItemId(91), vec![rate.clone()]),
            (ItemId(92), vec![rate.clone()]),
        ],
    )
    .unwrap();
    assert_eq!(
        creature.derived().health_recovery_period,
        period.div_ceil(2).max(1)
    );
    equipped(&mut creature, vec![(ItemId(92), vec![rate])]).unwrap();
    assert_eq!(
        creature.derived().health_recovery_period,
        period.div_ceil(2).max(1)
    );
    equipped(&mut creature, vec![]).unwrap();
    assert_eq!(creature.derived().health_recovery_period, period);
}

#[test]
fn signed_equipment_reduction_composes_for_every_damage_type_and_floors_at_zero() {
    let mut creature = creature();
    let categories = [
        DamageType::Energy,
        DamageType::Impact,
        DamageType::Keen,
        DamageType::Spirit,
        DamageType::Vital,
        DamageType::Psychic,
    ];
    let signed = |amount| {
        categories
            .iter()
            .map(|category| {
                modifier(ModifierOp::SignedReduction {
                    selector: Selector::Category {
                        category: *category,
                    },
                    amount,
                })
            })
            .collect()
    };
    equipped(&mut creature, vec![(ItemId(91), signed(-1))]).unwrap();
    for category in categories {
        assert_eq!(
            creature
                .derived()
                .protection
                .reduction(&Selector::Category { category }),
            0
        );
    }
    equipped(
        &mut creature,
        vec![(ItemId(91), signed(-1)), (ItemId(92), signed(2))],
    )
    .unwrap();
    for category in categories {
        assert_eq!(
            creature
                .derived()
                .protection
                .reduction(&Selector::Category { category }),
            1
        );
        let damage =
            DamageSpec::new(vec![DamageComponent::fixed(category, None, 3)], None).unwrap();
        assert_eq!(
            damage
                .resolve(
                    &mut 1,
                    Edge::default(),
                    &creature.derived().protection,
                    &mut ()
                )
                .total,
            2
        );
    }
    equipped(&mut creature, vec![]).unwrap();
    for category in categories {
        assert_eq!(
            creature
                .derived()
                .protection
                .reduction(&Selector::Category { category }),
            0
        );
    }
}
