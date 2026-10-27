//! A magical stick owns independent physical equipment and charged-use contracts.
use super::*;
use crate::combat::DamageType;
use crate::damage::{DamageComponent, DamageSpec};
use crate::{EquipmentSlot, EquipmentSlotId, EquipmentSpec};

fn stick_spec(staff: bool) -> ItemSpec {
    let component = if staff {
        DamageComponent::rolled(
            DamageType::Impact,
            None,
            crate::dice::DicePool::new(2, 3, 0).unwrap(),
        )
    } else {
        DamageComponent::fixed(DamageType::Impact, None, 1)
    };
    let primary = component.key();
    let mut spec = ItemSpec::ordinary("oak staff".into());
    spec.class = ItemClass::Wand;
    spec.magical = true;
    spec.identity = "fixture.physical_device".into();
    spec.name = "staff of embers".into();
    spec.concealed = true;
    spec.device = Some(DeviceSpec {
        ability: crate::content_ids::AbilityId::new("fixture_ray").unwrap(),
        initial_charges: ChargeRange {
            minimum: 3,
            maximum: 3,
        },
        discovery: ItemDiscovery::OnUse,
    });
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::new("weapon").unwrap(),
        periodic_relocation: None,
        armor: None,
        // Fixed overwhelming accuracy isolates damage/charge ownership from hit chance.
        attack: Some(
            crate::attacks::MeleeAttack::new(
                crate::attributes::Skill::HeavyWeaponry,
                100,
                60,
                40,
                DamageSpec::new(vec![component], Some(primary)).unwrap(),
            )
            .unwrap(),
        ),
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    spec
}

fn stick(game: &mut Game, staff: bool) -> ItemId {
    let at = game.actors[&ActorId(1)].location;
    let item = game
        .place_item_stack(10, at, Some(ActorId(1)), 1, stick_spec(staff))
        .unwrap();
    game.equip_authored(ActorId(1), item, EquipmentSlotId(0))
        .unwrap();
    item
}

#[test]
fn physical_devices_only_admit_plain_singleton_melee_equipment() {
    let game = fixture();
    for staff in [false, true] {
        let spec = stick_spec(staff);
        assert!(spec.valid());
        assert!(spec.validate_rules(game.content.rules()).is_ok());
        assert!(spec
            .equipment
            .as_ref()
            .unwrap()
            .supports_device(ItemClass::Wand));
        for mutation in 0..11 {
            let mut invalid = spec.clone();
            match mutation {
                0 => invalid.stackable = true,
                1 => invalid.class = ItemClass::Weapon,
                2 => invalid.equipment.as_mut().unwrap().slot = EquipmentSlot::new("ring").unwrap(),
                3 => invalid.equipment.as_mut().unwrap().attack = None,
                4 => {
                    invalid.equipment.as_mut().unwrap().armor = Some(crate::ArmorSpec {
                        physical_reduction: 1,
                        material: crate::ArmorMaterial::Metal,
                        rustproof: false,
                    })
                }
                5 => {
                    invalid.equipment.as_mut().unwrap().periodic_relocation =
                        Some(crate::PeriodicRelocation {
                            interval: 100,
                            chance_denominator: 2,
                        })
                }
                6 => invalid.supply = Some(crate::content_ids::SupplyId::new("ration").unwrap()),
                7 => invalid.currency = Some(crate::CurrencyKind::Gold),
                8 => {
                    invalid.projectile = Some(crate::projectile_profiles::ProjectileSpec {
                        family: crate::content_ids::LauncherFamilyId::new("fixture.bow").unwrap(),
                        damage: spec
                            .equipment
                            .as_ref()
                            .unwrap()
                            .attack
                            .as_ref()
                            .unwrap()
                            .damage()
                            .clone(),
                        recovery: crate::projectile_profiles::HitRecovery::Recoverable,
                    })
                }
                9 => invalid
                    .equipment
                    .as_mut()
                    .unwrap()
                    .modifiers
                    .push(crate::rules::Modifier {
                        cancellable: false,
                        scope: crate::rules::Scope::Always,
                        op: crate::rules::ModifierOp::Add {
                            stat: crate::rules::Stat::Health,
                            value: 1,
                        },
                    }),
                _ => {
                    invalid.equipment.as_mut().unwrap().weapon_profile =
                        Some(crate::weapon_profiles::WeaponProfileSpec {
                            expertise: crate::content_ids::ExpertiseId::new("fixture.weapon")
                                .unwrap(),
                            launcher: None,
                            thrown_payload: None,
                            base_traits: vec![],
                            expert_traits: crate::weapon_profiles::WeaponTraitChanges {
                                add: vec![],
                                remove: vec![],
                            },
                        })
                }
            }
            assert!(
                !invalid.valid(),
                "mutation {mutation} must not broaden hybrid capabilities"
            );
        }
    }
}

#[test]
fn physical_devices_keep_wand_class_and_do_not_bypass_worn_disclosure() {
    for staff in [false, true] {
        let mut game = fixture();
        execute(&mut game, Action::Move(crate::Direction::East));
        let before = game.observe(ActorId(1)).unwrap().worn;
        let item = stick(&mut game, staff);
        assert_eq!(game.items[&item].spec.class, ItemClass::Wand);
        assert_eq!(
            game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
            Some(&item)
        );
        let own = game.observe(ActorId(1)).unwrap();
        let carried = own.inventory.iter().find(|view| view.id == item).unwrap();
        assert_eq!(carried.class, ItemClass::Wand);
        assert_eq!(carried.name, "oak staff");
        assert!(!carried.identified);
        assert_eq!(own.worn, before);
        let seen = game.observe(ActorId(2)).unwrap();
        let views: Vec<_> = seen
            .visible_actors
            .iter()
            .filter(|view| view.id == ActorId(1))
            .collect();
        assert!(!views.is_empty(), "the observer sees the equipped actor");
        for view in views {
            assert_eq!(view.worn, before);
        }
        for view in [&own, &seen] {
            let public = format!("{view:?}");
            assert!(!public.contains("fixture.physical_device"));
            assert!(!public.contains("staff of embers"));
        }
        assert_eq!(game.items[&item].charges, Some(3));
        assert_eq!(game, roundtrip(&game));
    }
}

#[test]
fn staff_and_wand_melee_use_physical_payload_without_charges_or_identity_discovery() {
    for staff in [false, true] {
        let mut game = fixture();
        execute(&mut game, Action::Move(crate::Direction::East));
        let item = stick(&mut game, staff);
        let before = game.creature(ActorId(2)).unwrap().health().current();
        let plan = game
            .ability_plan(
                ActorId(1),
                &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                ActorId(2),
            )
            .unwrap();
        assert_eq!(plan.as_ability().unwrap().preparation, 60);
        assert_eq!(plan.as_ability().unwrap().recovery, 40);
        execute(&mut game, plan.work().action());
        let damage = before - game.creature(ActorId(2)).unwrap().health().current();
        if staff {
            assert!((2..=6).contains(&damage));
        } else {
            assert_eq!(damage, 1);
        }
        assert_eq!(game.items[&item].charges, Some(3));
        assert!(!game.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.physical_device"));
        assert!(game.actors[&ActorId(1)].known_charges.is_empty());
        assert!(game.items[&item].holds.is_empty());
        assert_eq!(game, roundtrip(&game));
        // The same equipped instance still uses the ordinary paid Zap path.
        execute(
            &mut game,
            Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North,
                },
            },
        );
        assert_eq!(game.items[&item].charges, Some(2));
        assert_eq!(
            game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
            Some(&item)
        );
        assert!(game.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.physical_device"));
        assert_eq!(
            game.creature(ActorId(2)).unwrap().health().current(),
            before - damage
        );
        assert_eq!(game, roundtrip(&game));
    }
}

#[test]
fn pending_equipped_device_keeps_exact_charge_hold_and_equipment_on_restore() {
    let mut game = fixture();
    let item = stick(&mut game, true);
    let plan = game
        .plan_work(
            ActorId(1),
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North,
                },
            },
        )
        .unwrap();
    start_owned_item_work(&mut game, plan.clone());
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, plan);
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(!game.items[&item].holds.is_empty());
    let before = game.clone();
    assert!(game
        .prepare_transfer(ActorId(1), item, None, false)
        .is_err());
    assert_eq!(game, before, "held equipment rejection must be atomic");
    let restored = roundtrip(&game);
    assert_eq!(restored, game);
    assert_eq!(
        restored
            .equipment(ActorId(1))
            .unwrap()
            .get(&EquipmentSlotId(0)),
        Some(&item)
    );
    assert!(restored.work_state_valid(ActorId(1)));
}

#[test]
fn device_curse_is_instance_owned_and_remove_curse_does_not_widen_weapon_enchantment() {
    let mut game = fixture();
    let item = stick(&mut game, true);
    game.items
        .edit(item, |value| value.condition.cursed = true)
        .unwrap();
    let condition = game.items[&item].condition;
    let rng = game.rules_rng;
    game.apply_effects(
        ActorId(1),
        &[crate::rules::Effect::AlterEquipment {
            selection: crate::EquippedSelection::Slot {
                slot: EquipmentSlot::new("weapon").unwrap(),
                class: ItemClass::Weapon,
                last: true,
            },
            alteration: crate::EquipmentAlteration::EnchantWeapon,
        }],
    )
    .unwrap();
    assert_eq!(game.items[&item].condition, condition);
    assert_eq!(game.rules_rng, rng);
    execute(&mut game, Action::Unequip { item });
    assert_eq!(
        game.equipment(ActorId(1)).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
    assert_eq!(
        game.actors[&ActorId(1)].known_equipment.curse(item),
        Some(true)
    );
    assert_eq!(game.actors[&ActorId(2)].known_equipment.curse(item), None);
    assert_eq!(game.items[&item].charges, Some(3));
    game.apply_effects(
        ActorId(1),
        &[crate::rules::Effect::AlterEquipment {
            selection: crate::EquippedSelection::All,
            alteration: crate::EquipmentAlteration::RemoveCurse,
        }],
    )
    .unwrap();
    assert!(!game.items[&item].condition.cursed);
    execute(&mut game, Action::Unequip { item });
    assert!(!game
        .equipment(ActorId(1))
        .unwrap()
        .values()
        .any(|id| *id == item));
    assert_eq!(game.items[&item].charges, Some(3));
    assert_eq!(game, roundtrip(&game));
}

#[test]
fn ordinary_actor_can_hold_physical_device_without_gaining_creature_only_zap() {
    let mut game = fixture();
    let actor = game
        .spawn_actor(at(4), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    // Ordinary actors are born without sockets. Give this actor the normal
    // equipment anatomy, not creature statistics or creature-only abilities.
    let anatomy = crate::AnatomySpec::from_rules(game.content.rules());
    game.configure_anatomy(actor, anatomy).unwrap();
    assert!(game.creature(actor).is_none());
    let item = game
        .place_item_stack(10, at(4), Some(actor), 1, stick_spec(true))
        .unwrap();
    game.equip_authored(actor, item, EquipmentSlotId(0))
        .unwrap();
    assert_eq!(
        game.equipment(actor).unwrap().get(&EquipmentSlotId(0)),
        Some(&item)
    );
    assert!(game
        .plan_work(
            actor,
            Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::North
                },
            }
        )
        .is_err());
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(game.items[&item].holds.is_empty());
    assert_eq!(game, roundtrip(&game));
}
