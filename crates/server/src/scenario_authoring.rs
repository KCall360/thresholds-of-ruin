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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentSlot {
    Weapon,
    BodyArmor,
    Shield,
    HeadArmor,
    HandsArmor,
    FeetArmor,
    Cloak,
    Ring,
    Amulet,
}
impl From<EquipmentSlot> for tor_simulation::EquipmentSlot {
    fn from(value: EquipmentSlot) -> Self {
        match value {
            EquipmentSlot::Weapon => Self::Weapon,
            EquipmentSlot::BodyArmor => Self::BodyArmor,
            EquipmentSlot::Shield => Self::Shield,
            EquipmentSlot::HeadArmor => Self::HeadArmor,
            EquipmentSlot::HandsArmor => Self::HandsArmor,
            EquipmentSlot::FeetArmor => Self::FeetArmor,
            EquipmentSlot::Cloak => Self::Cloak,
            EquipmentSlot::Ring => Self::Ring,
            EquipmentSlot::Amulet => Self::Amulet,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
}
impl From<AnatomySpec> for tor_simulation::AnatomySpec {
    fn from(value: AnatomySpec) -> Self {
        Self {
            slots: value.slots.into_iter().map(Into::into).collect(),
        }
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
        Ok(tor_simulation::EquipmentSpec {
            slot: self.slot.into(),
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
        components: Vec<DamageComponentExpr>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Damage {
        damage: DamageExpr,
    },
    Heal {
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
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<Effect>,
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
