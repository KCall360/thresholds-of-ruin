//! Author-owned declaration schemas. Conversions into simulation definitions are
//! explicit so runtime representation changes cannot silently alter package files.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemClass {
    #[default]
    Misc,
    Weapon,
    Armor,
    Potion,
    Food,
    Corpse,
    Tool,
    Amulet,
    Ring,
    Scroll,
    Spellbook,
    Wand,
    Coin,
    Gem,
}

impl From<ItemClass> for tor_simulation::ItemClass {
    fn from(value: ItemClass) -> Self {
        match value {
            ItemClass::Misc => Self::Misc,
            ItemClass::Weapon => Self::Weapon,
            ItemClass::Armor => Self::Armor,
            ItemClass::Potion => Self::Potion,
            ItemClass::Food => Self::Food,
            ItemClass::Corpse => Self::Corpse,
            ItemClass::Tool => Self::Tool,
            ItemClass::Amulet => Self::Amulet,
            ItemClass::Ring => Self::Ring,
            ItemClass::Scroll => Self::Scroll,
            ItemClass::Spellbook => Self::Spellbook,
            ItemClass::Wand => Self::Wand,
            ItemClass::Coin => Self::Coin,
            ItemClass::Gem => Self::Gem,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
}

impl From<DamageType> for tor_simulation::combat::DamageType {
    fn from(value: DamageType) -> Self {
        match value {
            DamageType::Energy => Self::Energy,
            DamageType::Impact => Self::Impact,
            DamageType::Keen => Self::Keen,
            DamageType::Spirit => Self::Spirit,
            DamageType::Vital => Self::Vital,
        }
    }
}

pub type EquipmentSlot = String;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
}
impl AnatomySpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::AnatomySpec, crate::Failure> {
        let slots = self
            .slots
            .iter()
            .map(|slot| {
                let id = tor_simulation::EquipmentSlot::new(slot)
                    .map_err(|_| super::fail(format!("Invalid equipment slot ID {slot:?}")))?;
                if rules.equipment_slot(&id).is_none() {
                    return Err(super::fail(format!("Unknown equipment slot {slot:?}")));
                }
                Ok(id)
            })
            .collect::<Result<Vec<_>, crate::Failure>>()?;
        let anatomy = tor_simulation::AnatomySpec { slots };
        if !anatomy.valid(rules) {
            return Err(super::fail("Invalid anatomy"));
        }
        Ok(anatomy)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSpec {
    pub slot: EquipmentSlot,
    pub attack: Option<crate::creature_authoring::Melee>,
    #[serde(default)]
    pub modifiers: Vec<crate::creature_authoring::Modifier>,
}
impl EquipmentSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::EquipmentSpec, crate::Failure> {
        let modifiers = self
            .modifiers
            .iter()
            .map(crate::creature_authoring::Modifier::compile)
            .collect::<Result<Vec<_>, _>>()?;
        rules
            .validate_modifiers(&modifiers)
            .map_err(|error| super::fail(format!("Invalid equipment modifier: {error:?}")))?;
        let slot = tor_simulation::EquipmentSlot::new(&self.slot)
            .map_err(|_| super::fail(format!("Invalid equipment slot ID {:?}", self.slot)))?;
        if rules.equipment_slot(&slot).is_none() {
            return Err(super::fail(format!(
                "Unknown equipment slot {:?}",
                self.slot
            )));
        }
        Ok(tor_simulation::EquipmentSpec {
            slot,
            attack: self
                .attack
                .as_ref()
                .map(|attack| attack.compile(rules))
                .transpose()?,
            modifiers,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiceExpr {
    pub count: u16,
    pub sides: u16,
    pub bonus: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AmountExpr {
    Fixed { amount: u32 },
    Rolled { dice: DiceExpr },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageComponentExpr {
    pub category: DamageType,
    pub descriptor: Option<String>,
    pub amount: AmountExpr,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DamageExpr {
    SelectedWeapon {
        impact_bonus: u32,
    },
    Components {
        #[serde(
            deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::DAMAGE_COMPONENTS }>"
        )]
        components: Vec<DamageComponentExpr>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    StepSelfOneCell,
    PushTargetOneCell {
        direction: PushDirection,
    },
    Damage {
        damage: DamageExpr,
    },
    Heal {
        amount: AmountExpr,
    },
    Nourish {
        amount: AmountExpr,
    },
    ApplyCondition {
        condition: String,
        duration: u64,
    },
    Restore {
        resource: String,
        amount: AmountExpr,
    },
    ObservePublicPlan,
    ApplyQualifiedConditionToSource {
        condition: String,
        duration: u64,
        ability_tag: String,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<Effect>,
}
impl<'de> Deserialize<'de> for ConsumableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(
                deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, _, { tor_simulation::limits::CONSUMABLE_EFFECTS }>"
            )]
            effects: Vec<Effect>,
        }
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a named consumable object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Fields, M::Error> {
                Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        let fields = deserializer.deserialize_map(Named)?;
        Ok(Self {
            effects: fields.effects,
        })
    }
}
impl ConsumableSpec {
    pub(crate) fn compile(
        &self,
        rules: &tor_simulation::rules::Ruleset,
    ) -> Result<tor_simulation::ConsumableSpec, crate::Failure> {
        let value =
            serde_json::to_value(&self.effects).map_err(|error| super::fail(error.to_string()))?;
        let effects: Vec<tor_simulation::rules::Effect> = serde_json::from_value(value)
            .map_err(|error| super::fail(format!("Invalid item effect: {error}")))?;
        rules
            .validate_effects(&effects)
            .map_err(|error| super::fail(format!("Invalid item effect: {error:?}")))?;
        Ok(tor_simulation::ConsumableSpec { effects })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodySpec {
    pub cells: Vec<[i32; 3]>,
    pub eye: [i32; 3],
    pub mass: u32,
}

impl From<BodySpec> for tor_simulation::BodySpec {
    fn from(value: BodySpec) -> Self {
        Self {
            cells: value.cells,
            eye: value.eye,
            mass: value.mass,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiProfile {
    pub memory_ticks: u64,
    pub flee_percent: u32,
}

impl Default for AiProfile {
    fn default() -> Self {
        Self {
            memory_ticks: 1000,
            flee_percent: 25,
        }
    }
}

impl From<AiProfile> for tor_simulation::ai::AiProfile {
    fn from(value: AiProfile) -> Self {
        Self {
            memory_ticks: value.memory_ticks,
            flee_percent: value.flee_percent,
        }
    }
}

impl<'de> Deserialize<'de> for Effect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            StepSelfOneCell {},
            PushTargetOneCell {
                direction: PushDirection,
            },
            Damage {
                damage: DamageExpr,
            },
            Heal {
                amount: AmountExpr,
            },
            Nourish {
                amount: AmountExpr,
            },
            ApplyCondition {
                condition: String,
                duration: u64,
            },
            Restore {
                resource: String,
                amount: AmountExpr,
            },
            ObservePublicPlan {},
            ApplyQualifiedConditionToSource {
                condition: String,
                duration: u64,
                ability_tag: String,
            },
        }
        struct Named;
        impl<'de> serde::de::Visitor<'de> for Named {
            type Value = Fields;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a named effect object")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Fields, M::Error> {
                Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }
        Ok(match deserializer.deserialize_map(Named)? {
            Fields::StepSelfOneCell {} => Self::StepSelfOneCell,
            Fields::PushTargetOneCell { direction } => Self::PushTargetOneCell { direction },
            Fields::Damage { damage } => Self::Damage { damage },
            Fields::Heal { amount } => Self::Heal { amount },
            Fields::Nourish { amount } => Self::Nourish { amount },
            Fields::ApplyCondition {
                condition,
                duration,
            } => Self::ApplyCondition {
                condition,
                duration,
            },
            Fields::Restore { resource, amount } => Self::Restore { resource, amount },
            Fields::ObservePublicPlan {} => Self::ObservePublicPlan,
            Fields::ApplyQualifiedConditionToSource {
                condition,
                duration,
                ability_tag,
            } => Self::ApplyQualifiedConditionToSource {
                condition,
                duration,
                ability_tag,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> std::sync::Arc<tor_simulation::rules::Ruleset> {
        tor_simulation::rules::Ruleset::validate(
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn equipment_slot_references_compile_from_data_and_reject_unknown_ids() {
        let mut document = rules().document().clone();
        document
            .equipment_slots
            .push(tor_simulation::rules::EquipmentSlotDef {
                id: tor_simulation::EquipmentSlot::new("tentacle").unwrap(),
                name: "Tentacle".into(),
                equipped_phrase: "held by a tentacle".into(),
                compatible_classes: vec![tor_simulation::ItemClass::Tool],
                requires_attack: false,
                equip_turns: 2,
                unequip_turns: 4,
            });
        let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
        let anatomy = AnatomySpec {
            slots: vec!["tentacle".into(), "tentacle".into()],
        };
        let compiled = anatomy.compile(&rules).unwrap();
        assert_eq!(compiled.slots[0].as_str(), "tentacle");
        let equipment = EquipmentSpec {
            slot: "tentacle".into(),
            attack: None,
            modifiers: vec![],
        };
        assert!(equipment
            .compile(&rules)
            .unwrap()
            .validate_rules(tor_simulation::ItemClass::Tool, &rules)
            .is_ok());
        for slot in ["missing_slot", "invalid slot"] {
            assert!(AnatomySpec {
                slots: vec![slot.into()]
            }
            .compile(&rules)
            .is_err());
            assert!(EquipmentSpec {
                slot: slot.into(),
                ..equipment.clone()
            }
            .compile(&rules)
            .is_err());
        }
    }

    #[test]
    fn generic_item_effects_compile_and_reject_unknown_content_references() {
        let valid = ConsumableSpec {
            effects: vec![Effect::Heal {
                amount: AmountExpr::Fixed { amount: 5 },
            }],
        };
        assert!(valid.compile(&rules()).is_ok());
        let missing = ConsumableSpec {
            effects: vec![Effect::ApplyCondition {
                condition: "missing_condition".into(),
                duration: 100,
            }],
        };
        assert!(missing.compile(&rules()).is_err());
        let missing_descriptor = ConsumableSpec {
            effects: vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: DamageType::Energy,
                        descriptor: Some("missing_descriptor".into()),
                        amount: AmountExpr::Fixed { amount: 5 },
                    }],
                },
            }],
        };
        assert!(missing_descriptor.compile(&rules()).is_err());
    }

    #[test]
    fn old_item_effect_and_equipment_shapes_are_rejected() {
        assert!(serde_json::from_value::<ConsumableSpec>(serde_json::json!({
            "effects": [{"type":"heal", "amount":5}]
        }))
        .is_err());
        assert!(serde_json::from_value::<EquipmentSpec>(serde_json::json!({
            "slot":"body_armor", "attack":null, "defense":1, "reductions":{}
        }))
        .is_err());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushDirection {
    AwayFromSource,
}
