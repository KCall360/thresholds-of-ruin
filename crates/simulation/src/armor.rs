//! Per-item armor protection. Definitions are shared; wear belongs to an item.
use crate::combat::DamageType;
use crate::rules::{Modifier, ModifierOp, Scope, Selector};
use crate::{EquipmentSlotId, Item, ItemId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmorMaterial {
    Metal,
    Nonmetal,
}
impl<'de> Deserialize<'de> for ArmorMaterial {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "metal" => Ok(Self::Metal),
            "nonmetal" => Ok(Self::Nonmetal),
            _ => Err(serde::de::Error::custom("invalid armor material")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArmorSpec {
    pub physical_reduction: u32,
    pub material: ArmorMaterial,
    pub rustproof: bool,
}
impl<'de> Deserialize<'de> for ArmorSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            physical_reduction: u32,
            material: ArmorMaterial,
            rustproof: bool,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let armor = Self {
            physical_reduction: fields.physical_reduction,
            material: fields.material,
            rustproof: fields.rustproof,
        };
        if !armor.valid() {
            return Err(serde::de::Error::custom("invalid armor reduction"));
        }
        Ok(armor)
    }
}
impl ArmorSpec {
    pub fn valid(self) -> bool {
        self.physical_reduction <= crate::limits::NUMERIC as u32
    }
    pub(crate) fn rustable(self) -> bool {
        self.material == ArmorMaterial::Metal && !self.rustproof
    }
}

impl crate::EquipmentSpec {
    pub(crate) fn modifiers_with_armor(&self, corrosion: u32) -> Option<Vec<Modifier>> {
        let mut modifiers = self.modifiers.clone();
        if let Some(armor) = self.armor {
            let amount = armor.physical_reduction.checked_sub(corrosion)?;
            if amount != 0 {
                for category in [DamageType::Impact, DamageType::Keen] {
                    modifiers.push(Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Reduction {
                            selector: Selector::Category { category },
                            amount,
                        },
                    });
                }
            }
        } else if corrosion != 0 {
            return None;
        }
        Some(modifiers)
    }
}

pub(crate) fn read_corrosion_amount<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<u32, D::Error> {
    let amount = u32::deserialize(d)?;
    if !(1..=crate::limits::NUMERIC as u32).contains(&amount) {
        return Err(serde::de::Error::custom("invalid corrosion amount"));
    }
    Ok(amount)
}

/// Exact owned condition, persisted with the item rather than its definition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemCondition {
    pub corrosion: u32,
}
impl<'de> Deserialize<'de> for ItemCondition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            corrosion: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.corrosion > crate::limits::NUMERIC as u32 {
            return Err(serde::de::Error::custom("invalid item corrosion"));
        }
        Ok(Self {
            corrosion: fields.corrosion,
        })
    }
}
impl Item {
    pub(crate) fn condition_valid(&self) -> bool {
        match self.spec.equipment.as_ref().and_then(|gear| gear.armor) {
            Some(armor) if armor.rustable() => self.condition.corrosion <= armor.physical_reduction,
            _ => self.condition.corrosion == 0,
        }
    }

    /// Identified equipment shows its current contribution, not unworn catalog values.
    pub(crate) fn equipment_view(&self) -> Option<crate::EquipmentSpec> {
        if !self.condition_valid() {
            return None;
        }
        let mut equipment = self.spec.equipment.clone()?;
        equipment.modifiers = equipment.modifiers_with_armor(self.condition.corrosion)?;
        equipment.armor = None;
        Some(equipment)
    }

    /// One contribution path for equip, previews, restoration and corrosion.
    /// The permanent compiled creature is retained by the ordinary equipment fold.
    pub(crate) fn equipment_source(
        &self,
        id: ItemId,
        socket: EquipmentSlotId,
    ) -> Option<crate::creatures::EquipmentSource> {
        if !self.condition_valid() {
            return None;
        }
        let equipment = self.equipment_view()?;
        Some(crate::creatures::EquipmentSource {
            item: id,
            socket,
            modifiers: equipment.modifiers,
            attack: equipment.attack,
        })
    }
}

impl crate::Game {
    /// Apply one armor occurrence to the first equipped socket of the authored kind.
    /// Both the exact item witness and compiled actor fold belong to the caller's
    /// transaction. A failure publishes neither, and never recompiles HD rows.
    pub(crate) fn corrode_armor_in_outcome(
        &mut self,
        outcome: &mut crate::lifecycle::ActorOutcome,
        slot: &crate::EquipmentSlot,
        requested: u32,
    ) -> Result<u32, crate::GameError> {
        if requested == 0 || requested > crate::limits::NUMERIC as u32 {
            return Err(crate::GameError::Rule(
                crate::abilities::RuleError::InvalidPlan,
            ));
        }
        if !outcome.state.alive() {
            return Ok(0);
        }
        let Some((_, item_id)) = outcome.state.equipment.iter().find(|(socket, _)| {
            outcome.state.anatomy.slots.get(usize::from(socket.0)) == Some(slot)
        }) else {
            return Ok(0);
        };
        let item_id = *item_id;
        let expected = self
            .items
            .get(&item_id)
            .ok_or(crate::GameError::ItemUnavailable)?
            .clone();
        if expected.location != crate::ItemLocation::Carried(outcome.id)
            || !expected.condition_valid()
        {
            return Err(crate::GameError::ItemUnavailable);
        }
        let Some(armor) = expected
            .spec
            .equipment
            .as_ref()
            .and_then(|gear| gear.armor)
            .filter(|armor| armor.rustable())
        else {
            return Ok(0);
        };
        let amount = requested.min(armor.physical_reduction - expected.condition.corrosion);
        if amount == 0 {
            return Ok(0);
        }
        let mut changed = expected.clone();
        changed.condition.corrosion += amount;
        let sources = outcome
            .state
            .equipment
            .iter()
            .map(|(socket, id)| {
                let item = if *id == item_id {
                    &changed
                } else {
                    self.items
                        .get(id)
                        .ok_or(crate::GameError::ItemUnavailable)?
                };
                item.equipment_source(*id, *socket)
                    .ok_or(crate::GameError::ItemUnavailable)
            })
            .collect::<Result<Vec<_>, crate::GameError>>()?;
        let context = self.equipment_recovery_context(outcome.id, &sources)?;
        if let Some(combat) = outcome.state.combat.as_mut() {
            combat
                .set_equipment_in_context(sources, context)
                .map_err(crate::GameError::Creature)?;
        }
        if self.items.get(&item_id) != Some(&expected) {
            return Err(crate::GameError::ItemUnavailable);
        }
        self.items
            .edit(item_id, |item| item.condition = changed.condition)
            .ok_or(crate::GameError::ItemUnavailable)?;
        Ok(amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EquipmentSpec, ItemSpec};

    fn item(armor: ArmorSpec) -> Item {
        let mut spec = ItemSpec::ordinary("mail".into());
        spec.class = crate::ItemClass::Armor;
        spec.equipment = Some(EquipmentSpec {
            slot: crate::EquipmentSlot::new("body_armor").unwrap(),
            armor: Some(armor),
            attack: None,
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Reduction {
                    selector: Selector::Category {
                        category: DamageType::Energy,
                    },
                    amount: 7,
                },
            }],
        });
        Item {
            corpse: None,
            condition: ItemCondition::default(),
            charges: None,
            motion: Default::default(),
            orientation: 0,
            spec: tor_world::Shared::new(spec),
            quantity: 1,
            holds: vec![],
            location: crate::ItemLocation::Carried(crate::ActorId(1)),
        }
    }
    fn metal() -> ArmorSpec {
        ArmorSpec {
            physical_reduction: 2,
            material: ArmorMaterial::Metal,
            rustproof: false,
        }
    }
    #[test]
    fn item_owned_corrosion_changes_only_its_physical_reduction() {
        let original = item(metal());
        let mut worn = original.clone();
        for loss in 0..=2 {
            worn.condition.corrosion = loss;
            let source = worn
                .equipment_source(ItemId(1), EquipmentSlotId(0))
                .unwrap();
            let reductions = source
                .modifiers
                .iter()
                .map(|modifier| match modifier.op {
                    ModifierOp::Reduction {
                        selector: Selector::Category { category },
                        amount,
                    } => (category, amount),
                    _ => panic!("armor must not change defense"),
                })
                .collect::<Vec<_>>();
            let mut expected = vec![(DamageType::Energy, 7)];
            if loss < 2 {
                expected.extend([(DamageType::Impact, 2 - loss), (DamageType::Keen, 2 - loss)]);
            }
            assert_eq!(reductions, expected);
            assert_eq!(original.condition.corrosion, 0);
            assert_eq!(original.spec, worn.spec);
        }
        worn.condition.corrosion = 3;
        assert!(!worn.condition_valid());
        assert!(worn
            .equipment_source(ItemId(1), EquipmentSlotId(0))
            .is_none());
    }
    #[test]
    fn nonmetal_and_rustproof_cannot_restore_with_corrosion() {
        for armor in [
            ArmorSpec {
                material: ArmorMaterial::Nonmetal,
                ..metal()
            },
            ArmorSpec {
                rustproof: true,
                ..metal()
            },
        ] {
            let mut item = item(armor);
            assert!(item.condition_valid());
            item.condition.corrosion = 1;
            assert!(!item.condition_valid());
        }
    }
    #[test]
    fn owned_armor_and_condition_records_reject_missing_unknown_and_sequence_shapes() {
        for value in [
            serde_json::json!({}),
            serde_json::json!({"corrosion":0,"extra":0}),
            serde_json::json!([0]),
            serde_json::json!({"corrosion":1000001}),
        ] {
            assert!(serde_json::from_value::<ItemCondition>(value).is_err());
        }
        for value in [
            serde_json::json!({"physical_reduction":2,"material":"metal"}),
            serde_json::json!({"physical_reduction":2,"material":"metal","rustproof":false,"extra":0}),
            serde_json::json!([2, "metal", false]),
        ] {
            assert!(serde_json::from_value::<ArmorSpec>(value).is_err());
        }
        let raw = serde_json::to_value(item(metal())).unwrap();
        let mut missing = raw.clone();
        missing.as_object_mut().unwrap().remove("condition");
        assert!(serde_json::from_value::<Item>(missing).is_err());
        assert!(serde_json::from_value::<Item>(raw).is_ok());
    }
}
