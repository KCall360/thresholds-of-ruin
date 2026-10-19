//! Explicit bounded birth policy. Sampling owns condition, never archetype data.
use crate::{armor::ItemCondition, EquipmentSpec, GameError, ItemClass};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EquipmentGeneration {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn armor() -> EquipmentSpec {
        EquipmentSpec {
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
            .sample(ItemClass::Armor, Some(&gear), &mut rng)
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
            .sample(ItemClass::Armor, Some(&gear), &mut rng)
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
                .sample(ItemClass::Armor, Some(&gear), &mut first)
                .unwrap();
            assert_eq!(
                condition,
                policy
                    .sample(ItemClass::Armor, Some(&gear), &mut second)
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
        assert!(!policy.valid_for(ItemClass::Weapon, Some(&gear)));
        assert!(!policy.valid_for(ItemClass::Armor, None));
        let mut extreme = gear;
        extreme.armor.as_mut().unwrap().physical_reduction = crate::limits::NUMERIC as u32;
        assert!(!policy.valid_for(ItemClass::Armor, Some(&extreme)));
        let mut zero = armor();
        zero.armor.as_mut().unwrap().physical_reduction = 0;
        let cursed = EquipmentGeneration::Armor {
            curse_percent: 100,
            enchant_percent: 0,
            minimum: 1,
            maximum: 3,
        };
        let condition = cursed
            .sample(ItemClass::Armor, Some(&zero), &mut first)
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
        .sample(ItemClass::Weapon, Some(&gear), &mut rng)
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
                .sample(ItemClass::Weapon, Some(&gear), &mut rng)
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
        assert!(!invalid.valid_for(ItemClass::Weapon, Some(&gear)));
        let before = rng;
        assert!(invalid
            .sample(ItemClass::Weapon, Some(&gear), &mut rng)
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

impl<'de> Deserialize<'de> for EquipmentGeneration {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
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
    pub fn valid_for(self, class: ItemClass, equipment: Option<&EquipmentSpec>) -> bool {
        let Some(gear) = equipment else {
            return false;
        };
        if !self.valid() {
            return false;
        }
        match self {
            Self::Authored {
                hit_enchantment,
                damage_enchantment,
            } => match gear.attack.as_ref() {
                Some(attack) => attack
                    .with_enchantment(hit_enchantment, damage_enchantment)
                    .is_ok(),
                None => hit_enchantment == 0 && damage_enchantment == 0,
            },
            Self::Weapon {
                curse_percent,
                enchant_percent,
                maximum,
                ..
            } => {
                class == ItemClass::Weapon
                    && gear.attack.as_ref().is_some_and(|attack| {
                        (curse_percent == 0
                            || attack.with_enchantment(-(maximum as i32), 0).is_ok())
                            && (enchant_percent == 0
                                || attack.with_enchantment(maximum as i32, 0).is_ok())
                    })
            }
            Self::Armor {
                enchant_percent,
                maximum,
                ..
            } => {
                class == ItemClass::Armor
                    && gear.armor.is_some_and(|armor| {
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
        rng: &mut u64,
    ) -> Result<ItemCondition, GameError> {
        if !self.valid_for(class, equipment) {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        let (curse_percent, enchant_percent, minimum, maximum) = match self {
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
                Self::Authored { .. } => unreachable!("authored policy returned before sampling"),
            }
        }
        *rng = candidate;
        Ok(condition)
    }
}
