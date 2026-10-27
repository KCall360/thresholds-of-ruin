//! Explicit bounded birth policy. Sampling owns condition, never archetype data.
use crate::{armor::ItemCondition, EquipmentSpec, GameError, ItemClass};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EquipmentGeneration {
    /// Guaranteed ordinary curse, independent of enchantment or item class.
    Cursed,
    /// Uniform nonzero signed potency; negative outcomes start cursed.
    SignedPotency { negative: u32, positive: u32 },
    Authored {
        hit_enchantment: i32,
        damage_enchantment: i32,
    },
    Weapon {
        curse_percent: u8,
        enchant_percent: u8,
        minimum: u32,
        maximum: u32,
    },
    Armor {
        curse_percent: u8,
        enchant_percent: u8,
        minimum: u32,
        maximum: u32,
    },
}

impl<'de> Deserialize<'de> for EquipmentGeneration {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Cursed {},
            SignedPotency {
                negative: u32,
                positive: u32,
            },
            Authored {
                hit_enchantment: i32,
                damage_enchantment: i32,
            },
            Weapon {
                curse_percent: u8,
                enchant_percent: u8,
                minimum: u32,
                maximum: u32,
            },
            Armor {
                curse_percent: u8,
                enchant_percent: u8,
                minimum: u32,
                maximum: u32,
            },
        }
        let value = match Fields::deserialize(d)? {
            Fields::Cursed {} => Self::Cursed,
            Fields::SignedPotency { negative, positive } => {
                Self::SignedPotency { negative, positive }
            }
            Fields::Authored {
                hit_enchantment,
                damage_enchantment,
            } => Self::Authored {
                hit_enchantment,
                damage_enchantment,
            },
            Fields::Weapon {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            } => Self::Weapon {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            },
            Fields::Armor {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            } => Self::Armor {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            },
        };
        if !value.valid() {
            return Err(serde::de::Error::custom(
                "invalid equipment generation policy",
            ));
        }
        Ok(value)
    }
}

impl EquipmentGeneration {
    pub fn valid(self) -> bool {
        match self {
            Self::Cursed => true,
            Self::SignedPotency { negative, positive } => {
                negative <= crate::limits::NUMERIC as u32
                    && positive <= crate::limits::NUMERIC as u32
                    && negative
                        .checked_add(positive)
                        .is_some_and(|total| total > 0)
            }
            Self::Authored {
                hit_enchantment,
                damage_enchantment,
            } => [hit_enchantment, damage_enchantment]
                .into_iter()
                .all(|amount| {
                    (-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(amount))
                }),
            Self::Weapon {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            }
            | Self::Armor {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            } => {
                u16::from(curse_percent) + u16::from(enchant_percent) <= 100
                    && minimum > 0
                    && minimum <= maximum
                    && maximum <= crate::limits::NUMERIC as u32
            }
        }
    }

    /// Validate every possible outcome before accepting a definition. In
    /// particular a low-probability invalid bonus must not make births flaky.
    pub fn valid_for(
        self,
        class: ItemClass,
        equipment: Option<&EquipmentSpec>,
        projectile: Option<&crate::projectile_profiles::ProjectileSpec>,
    ) -> bool {
        if !self.valid()
            || (equipment.is_none() && projectile.is_none())
            || projectile.is_some_and(|projectile| projectile.validate_shape().is_err())
        {
            return false;
        }
        match self {
            Self::Cursed => equipment.is_some() && projectile.is_none(),
            Self::SignedPotency { negative, positive } => {
                projectile.is_none()
                    && equipment.is_some_and(|gear| {
                        !gear.potency_modifiers.is_empty()
                            && gear.scaled_potency_modifiers(-(negative as i32)).is_some()
                            && gear.scaled_potency_modifiers(positive as i32).is_some()
                    })
            }
            Self::Authored {
                hit_enchantment,
                damage_enchantment,
            } => ItemCondition {
                hit_enchantment,
                damage_enchantment,
                ..Default::default()
            }
            .weapon_capabilities_valid(equipment, projectile),
            Self::Weapon {
                curse_percent,
                enchant_percent,
                maximum,
                ..
            } => {
                class == ItemClass::Weapon
                    && (equipment.is_some_and(|gear| gear.attack.is_some()) || projectile.is_some())
                    && (curse_percent == 0
                        || ItemCondition {
                            hit_enchantment: -(maximum as i32),
                            ..Default::default()
                        }
                        .weapon_capabilities_valid(equipment, projectile))
                    && (enchant_percent == 0
                        || ItemCondition {
                            hit_enchantment: maximum as i32,
                            ..Default::default()
                        }
                        .weapon_capabilities_valid(equipment, projectile))
            }
            Self::Armor {
                enchant_percent,
                maximum,
                ..
            } => {
                class == ItemClass::Armor
                    && projectile.is_none()
                    && equipment.and_then(|gear| gear.armor).is_some_and(|armor| {
                        armor.valid()
                            && (enchant_percent == 0
                                || ItemCondition {
                                    armor_enchantment: maximum,
                                    ..Default::default()
                                }
                                .effective_armor(armor)
                                .is_some())
                    })
            }
        }
    }

    pub(crate) fn sample(
        self,
        class: ItemClass,
        equipment: Option<&EquipmentSpec>,
        projectile: Option<&crate::projectile_profiles::ProjectileSpec>,
        rng: &mut u64,
    ) -> Result<ItemCondition, GameError> {
        if !self.valid_for(class, equipment, projectile) {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        let (curse_percent, enchant_percent, minimum, maximum) = match self {
            Self::Cursed => {
                return Ok(ItemCondition {
                    cursed: true,
                    ..Default::default()
                })
            }
            Self::SignedPotency { negative, positive } => {
                let mut candidate = *rng;
                let roll = crate::quantity::QuantityRange::new(
                    1,
                    u64::from(negative) + u64::from(positive),
                )?
                .sample(&mut candidate)?;
                let potency = if roll <= u64::from(negative) {
                    -(roll as i32)
                } else {
                    (roll - u64::from(negative)) as i32
                };
                *rng = candidate;
                return Ok(ItemCondition {
                    potency,
                    cursed: potency < 0,
                    ..Default::default()
                });
            }
            Self::Authored {
                hit_enchantment,
                damage_enchantment,
            } => {
                return Ok(ItemCondition {
                    hit_enchantment,
                    damage_enchantment,
                    ..Default::default()
                })
            }
            Self::Weapon {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            }
            | Self::Armor {
                curse_percent,
                enchant_percent,
                minimum,
                maximum,
            } => (curse_percent, enchant_percent, minimum, maximum),
        };
        let mut candidate = *rng;
        let roll = crate::quantity::QuantityRange::new(1, 100)?.sample(&mut candidate)?;
        let cursed = roll <= u64::from(curse_percent);
        let enchanted = !cursed && roll <= u64::from(curse_percent) + u64::from(enchant_percent);
        let mut condition = ItemCondition::default();
        if cursed || enchanted {
            let amount =
                crate::quantity::QuantityRange::new(u64::from(minimum), u64::from(maximum))?
                    .sample(&mut candidate)? as u32;
            condition.cursed = cursed;
            match self {
                Self::Weapon { .. } => {
                    condition.hit_enchantment = if cursed {
                        -(amount as i32)
                    } else {
                        amount as i32
                    }
                }
                Self::Armor { .. } if cursed => {
                    // TOR adaptation: negative physical protection floors at zero;
                    // curse remains independent even for zero-DR armor.
                    condition.armor_penalty = amount.min(
                        equipment
                            .and_then(|gear| gear.armor)
                            .expect("validated armor")
                            .physical_reduction,
                    );
                }
                Self::Armor { .. } => condition.armor_enchantment = amount,
                Self::Cursed | Self::Authored { .. } | Self::SignedPotency { .. } => {
                    unreachable!("direct policy returned before sampling")
                }
            }
        }
        *rng = candidate;
        Ok(condition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armor() -> EquipmentSpec {
        EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            slot: crate::EquipmentSlot::new("body_armor").unwrap(),
            armor: Some(crate::ArmorSpec {
                physical_reduction: 2,
                material: crate::ArmorMaterial::Metal,
                rustproof: false,
            }),
            attack: None,
            modifiers: vec![],
        }
    }

    #[test]
    fn guaranteed_curse_is_generic_and_draws_no_randomness() {
        let gear = armor();
        let policy: EquipmentGeneration = serde_json::from_str(r#"{"kind":"cursed"}"#).unwrap();
        let mut rng = 42;
        assert_eq!(
            policy
                .sample(ItemClass::Armor, Some(&gear), None, &mut rng)
                .unwrap(),
            ItemCondition {
                cursed: true,
                ..Default::default()
            }
        );
        assert_eq!(rng, 42);
        assert!(!policy.valid_for(ItemClass::Ring, None, None));
        assert!(
            serde_json::from_str::<EquipmentGeneration>(r#"{"kind":"cursed","potency":-1}"#)
                .is_err()
        );
    }

    #[test]
    fn bounded_policy_rejects_invalid_probabilities_ranges_and_unknown_fields() {
        for value in [
            serde_json::json!({"kind":"armor","curse_percent":101,"enchant_percent":0,"minimum":1,"maximum":3}),
            serde_json::json!({"kind":"weapon","curse_percent":80,"enchant_percent":21,"minimum":1,"maximum":3}),
            serde_json::json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":0,"maximum":3}),
            serde_json::json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":4,"maximum":3}),
            serde_json::json!({"kind":"authored","minimum":1}),
            serde_json::json!({"kind":"authored"}),
            serde_json::json!({"kind":"authored","hit_enchantment":0}),
            serde_json::json!({"kind":"authored","hit_enchantment":1000001,"damage_enchantment":0}),
            serde_json::json!({"kind":"armor","curse_percent":20,"enchant_percent":8,"minimum":1,"maximum":1000001}),
        ] {
            assert!(serde_json::from_value::<EquipmentGeneration>(value).is_err());
        }
    }

    #[test]
    fn authored_bypasses_rng_and_armor_penalty_is_clamped_without_losing_curse() {
        let gear = armor();
        let mut rng = 123;
        assert_eq!(
            EquipmentGeneration::Authored {
                hit_enchantment: 0,
                damage_enchantment: 0
            }
            .sample(ItemClass::Armor, Some(&gear), None, &mut rng)
            .unwrap(),
            ItemCondition::default()
        );
        assert_eq!(rng, 123);
        let policy = EquipmentGeneration::Armor {
            curse_percent: 100,
            enchant_percent: 0,
            minimum: 3,
            maximum: 3,
        };
        let condition = policy
            .sample(ItemClass::Armor, Some(&gear), None, &mut rng)
            .unwrap();
        assert!(condition.cursed);
        assert_eq!(condition.armor_penalty, 2);
        assert_eq!(condition.armor_enchantment, 0);
    }

    #[test]
    fn armor_generation_is_reproducible_and_confined_to_disjoint_outcomes() {
        let gear = armor();
        let policy = EquipmentGeneration::Armor {
            curse_percent: 20,
            enchant_percent: 8,
            minimum: 1,
            maximum: 3,
        };
        let (mut first, mut second) = (42, 42);
        let mut outcomes = [false; 3];
        for _ in 0..1000 {
            let condition = policy
                .sample(ItemClass::Armor, Some(&gear), None, &mut first)
                .unwrap();
            assert_eq!(
                condition,
                policy
                    .sample(ItemClass::Armor, Some(&gear), None, &mut second)
                    .unwrap()
            );
            if condition.cursed {
                outcomes[0] = true;
                assert!((1..=2).contains(&condition.armor_penalty));
                assert_eq!(condition.armor_enchantment, 0);
            } else if condition.armor_enchantment != 0 {
                outcomes[1] = true;
                assert!((1..=3).contains(&condition.armor_enchantment));
                assert_eq!(condition.armor_penalty, 0);
            } else {
                outcomes[2] = true;
                assert_eq!(condition, ItemCondition::default());
            }
        }
        assert_eq!(outcomes, [true; 3]);
        assert_eq!(first, second);
        assert!(!policy.valid_for(ItemClass::Weapon, Some(&gear), None));
        assert!(!policy.valid_for(ItemClass::Armor, None, None));
        let mut extreme = gear;
        extreme.armor.as_mut().unwrap().physical_reduction = crate::limits::NUMERIC as u32;
        assert!(!policy.valid_for(ItemClass::Armor, Some(&extreme), None));
        let mut zero = armor();
        zero.armor.as_mut().unwrap().physical_reduction = 0;
        let cursed = EquipmentGeneration::Armor {
            curse_percent: 100,
            enchant_percent: 0,
            minimum: 1,
            maximum: 3,
        };
        let condition = cursed
            .sample(ItemClass::Armor, Some(&zero), None, &mut first)
            .unwrap();
        assert!(condition.cursed);
        assert_eq!(condition.armor_penalty, 0);
    }

    fn weapon() -> EquipmentSpec {
        let component = crate::damage::DamageComponent::rolled(
            crate::combat::DamageType::Impact,
            None,
            crate::dice::DicePool::new(2, 4, 0).unwrap(),
        );
        let primary = component.key();
        EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            slot: crate::EquipmentSlot::new("weapon").unwrap(),
            armor: None,
            attack: Some(
                crate::attacks::MeleeAttack::new(
                    crate::attributes::Skill::HeavyWeaponry,
                    0,
                    60,
                    40,
                    crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
                )
                .unwrap(),
            ),
            modifiers: vec![],
        }
    }

    #[test]
    fn authored_starting_mace_preserves_effective_stats_without_randomness() {
        let gear = weapon();
        let base = gear.attack.as_ref().unwrap();
        let mut rng = 42;
        let condition = EquipmentGeneration::Authored {
            hit_enchantment: 1,
            damage_enchantment: 1,
        }
        .sample(ItemClass::Weapon, Some(&gear), None, &mut rng)
        .unwrap();
        assert_eq!(rng, 42);
        assert!(!condition.cursed);
        let effective = base
            .with_enchantment(condition.hit_enchantment, condition.damage_enchantment)
            .unwrap();
        assert_eq!(effective.bonus(), 1);
        assert_eq!(effective.damage_enchantment(), 1);
        assert_eq!(effective.damage(), base.damage());
        assert_eq!(base.bonus(), 0);
        assert_eq!(base.damage_enchantment(), 0);
    }

    #[test]
    fn weapon_birth_changes_hit_only_and_checks_all_possible_outcomes() {
        let gear = weapon();
        let mut seen = [false; 3];
        let mut rng = 42;
        let policy = EquipmentGeneration::Weapon {
            curse_percent: 10,
            enchant_percent: 5,
            minimum: 1,
            maximum: 3,
        };
        for _ in 0..1000 {
            let condition = policy
                .sample(ItemClass::Weapon, Some(&gear), None, &mut rng)
                .unwrap();
            assert_eq!(condition.damage_enchantment, 0);
            assert_eq!(condition.armor_enchantment, 0);
            assert!((-3..=3).contains(&condition.hit_enchantment));
            assert_eq!(condition.cursed, condition.hit_enchantment < 0);
            seen[if condition.cursed {
                0
            } else if condition.hit_enchantment > 0 {
                1
            } else {
                2
            }] = true;
        }
        assert_eq!(seen, [true; 3]);
        let invalid = EquipmentGeneration::Weapon {
            curse_percent: 1,
            enchant_percent: 0,
            minimum: 1,
            maximum: crate::limits::NUMERIC as u32,
        };
        assert!(!invalid.valid_for(ItemClass::Weapon, Some(&gear), None));
        let before = rng;
        assert!(invalid
            .sample(ItemClass::Weapon, Some(&gear), None, &mut rng)
            .is_err());
        assert_eq!(rng, before);
    }

    #[test]
    fn birth_replay_and_failed_placement_preserve_owned_state_and_rng() {
        let mut game = crate::Game::two_room(42, crate::test_creatures::content());
        let at = tor_world::Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        // A restorable game needs a scheduled actor; the geometry-only fixture
        // deliberately starts empty. Keep the ready actor at the current tick.
        game.spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut spec = crate::ItemSpec::ordinary("generated mail".into());
        spec.class = ItemClass::Armor;
        spec.equipment = Some(armor());
        spec.equipment_generation = Some(EquipmentGeneration::Armor {
            curse_percent: 20,
            enchant_percent: 8,
            minimum: 1,
            maximum: 3,
        });
        let before = game.clone();
        assert!(game
            .place_item_stack(100, at, Some(crate::ActorId(999)), 1, spec.clone())
            .is_err());
        assert_eq!(game, before);
        let first = game
            .place_item_stack(100, at, None, 1, spec.clone())
            .unwrap();
        assert_eq!(*game.items[&first].spec, spec);
        let after_birth = game.clone();
        assert!(game
            .place_item_stack(100, at, None, 1, spec.clone())
            .is_err());
        assert_eq!(game, after_birth);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            crate::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        for id in 101..120 {
            game.place_item_stack(id, at, None, 1, spec.clone())
                .unwrap();
            restored
                .place_item_stack(id, at, None, 1, spec.clone())
                .unwrap();
            assert_eq!(game, restored);
        }
        assert_eq!(
            game.items[&first].condition,
            after_birth.items[&first].condition
        );
    }
}

#[cfg(test)]
mod potency_tests {
    use super::*;
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};

    fn gear() -> EquipmentSpec {
        EquipmentSpec {
            periodic_relocation: None,
            slot: crate::EquipmentSlot::new("ring").unwrap(),
            armor: None,
            attack: None,
            weapon_profile: None,
            modifiers: vec![],
            potency_modifiers: vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::AttackDamageRollBonus,
                    value: 1,
                },
            }],
        }
    }

    #[test]
    fn signed_birth_has_three_exact_outcomes_and_negative_curse() {
        let policy = EquipmentGeneration::SignedPotency {
            negative: 1,
            positive: 2,
        };
        let gear = gear();
        let before = gear.clone();
        let mut counts = std::collections::BTreeMap::new();
        for mut seed in 1..301 {
            let condition = policy
                .sample(ItemClass::Ring, Some(&gear), None, &mut seed)
                .unwrap();
            assert!([-1, 1, 2].contains(&condition.potency));
            assert_eq!(condition.cursed, condition.potency < 0);
            *counts.entry(condition.potency).or_insert(0) += 1;
        }
        assert_eq!(counts.len(), 3);
        assert_eq!(gear, before);
    }

    #[test]
    fn potency_scaling_is_signed_and_rejects_boolean_or_overflow() {
        let mut gear = gear();
        assert!(matches!(
            gear.scaled_potency_modifiers(-1).unwrap()[0].op,
            ModifierOp::Add { value: -1, .. }
        ));
        gear.potency_modifiers[0].op = ModifierOp::ProtectArmor {};
        assert!(gear.scaled_potency_modifiers(-1).is_none());
        gear.potency_modifiers[0].op = ModifierOp::Add {
            stat: Stat::AttackDamageRollBonus,
            value: crate::limits::NUMERIC,
        };
        assert!(gear.scaled_potency_modifiers(2).is_none());
    }

    #[test]
    fn invalid_potency_policy_rolls_back_rng_and_strict_roundtrip() {
        let policy = EquipmentGeneration::SignedPotency {
            negative: 0,
            positive: 0,
        };
        let mut rng = 17;
        assert!(policy
            .sample(ItemClass::Ring, Some(&gear()), None, &mut rng)
            .is_err());
        assert_eq!(rng, 17);
        let policy = EquipmentGeneration::SignedPotency {
            negative: 1,
            positive: 2,
        };
        let wire = serde_json::to_value(policy).unwrap();
        assert_eq!(
            serde_json::from_value::<EquipmentGeneration>(wire.clone()).unwrap(),
            policy
        );
        for field in ["negative", "positive"] {
            let mut bad = wire.clone();
            bad.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<EquipmentGeneration>(bad).is_err());
        }
        let mut bad = wire;
        bad["extra"] = serde_json::json!(0);
        assert!(serde_json::from_value::<EquipmentGeneration>(bad).is_err());
    }
}
