//! Frozen equipped-item alterations. Selection belongs to paid preparation;
//! instance mutation and the derived equipment fold publish in one transaction.
use crate::{
    ActorId, EquipmentSlot, EquipmentSlotId, Game, GameError, ItemClass, ItemId, ItemLocation,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentAlteration {
    EnchantArmor,
    EnchantWeapon,
    ProtectArmor,
    RemoveCurse,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EquippedSelection {
    Slot {
        slot: EquipmentSlot,
        class: ItemClass,
        last: bool,
    },
    All,
}
impl<'de> Deserialize<'de> for EquippedSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Slot {
                slot: EquipmentSlot,
                class: ItemClass,
                last: bool,
            },
            All {},
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Slot { slot, class, last } => Self::Slot { slot, class, last },
            Fields::All {} => Self::All,
        })
    }
}
impl EquippedSelection {
    pub(crate) fn accepts(&self, alteration: EquipmentAlteration) -> bool {
        match alteration {
            EquipmentAlteration::EnchantArmor | EquipmentAlteration::ProtectArmor => matches!(
                self,
                Self::Slot {
                    class: ItemClass::Armor,
                    ..
                }
            ),
            EquipmentAlteration::EnchantWeapon => matches!(
                self,
                Self::Slot {
                    class: ItemClass::Weapon,
                    ..
                }
            ),
            EquipmentAlteration::RemoveCurse => true,
        }
    }
    pub(crate) fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        match self {
            Self::All => true,
            Self::Slot { slot, class, .. } => rules
                .equipment_slot(slot)
                .is_some_and(|s| s.compatible_classes.contains(class)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EquippedWitness {
    pub(crate) item: ItemId,
    pub(crate) socket: EquipmentSlotId,
    pub(crate) condition: crate::armor::ItemCondition,
}
impl<'de> Deserialize<'de> for EquippedWitness {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            socket: EquipmentSlotId,
            condition: crate::armor::ItemCondition,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.item.0 == 0
            || f.item.0 == u64::MAX
            || usize::from(f.socket.0) >= crate::limits::EQUIPMENT_SOCKETS
        {
            return Err(serde::de::Error::custom("invalid equipped witness"));
        }
        Ok(Self {
            item: f.item,
            socket: f.socket,
            condition: f.condition,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedEquipmentAlteration {
    pub selection: EquippedSelection,
    pub alteration: EquipmentAlteration,
    pub(crate) targets: Vec<EquippedWitness>,
}
impl<'de> Deserialize<'de> for PlannedEquipmentAlteration {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            selection: EquippedSelection,
            alteration: EquipmentAlteration,
            targets: crate::bounded::Bounded<EquippedWitness, { crate::limits::EQUIPMENT_SOCKETS }>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            selection: f.selection,
            alteration: f.alteration,
            targets: f.targets.0,
        };
        if !value.valid_shape() {
            return Err(serde::de::Error::custom("invalid equipment alteration"));
        }
        Ok(value)
    }
}
impl PlannedEquipmentAlteration {
    pub(crate) fn valid_shape(&self) -> bool {
        self.selection.accepts(self.alteration)
            && self.targets.len() <= crate::limits::EQUIPMENT_SOCKETS
            && (matches!(self.selection, EquippedSelection::All) || self.targets.len() <= 1)
            && self.targets.windows(2).all(|w| w[0].socket < w[1].socket)
            && self.targets.iter().all(|w| {
                w.condition.valid()
                    && w.item.0 > 0
                    && w.item.0 < u64::MAX
                    && usize::from(w.socket.0) < crate::limits::EQUIPMENT_SOCKETS
            })
            && self
                .targets
                .iter()
                .map(|w| w.item)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.targets.len()
    }
    pub(crate) fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        self.valid_shape() && self.selection.valid(rules)
    }
}
/// The random choice is frozen by resolution, never resampled by publication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedEquipmentAlteration {
    pub plan: PlannedEquipmentAlteration,
    pub hit: bool,
}
impl<'de> Deserialize<'de> for ResolvedEquipmentAlteration {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            plan: PlannedEquipmentAlteration,
            hit: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        if f.hit
            && (f.plan.alteration != EquipmentAlteration::EnchantWeapon
                || f.plan.targets.is_empty())
        {
            return Err(serde::de::Error::custom("unexpected weapon choice"));
        }
        Ok(Self {
            plan: f.plan,
            hit: f.hit,
        })
    }
}
impl crate::Item {
    /// Authored and owned magic both count; damage alone never makes gear magical.
    pub(crate) fn magical(&self) -> bool {
        self.spec.magical
            || self.condition.cursed
            || self.condition.protected
            || self.condition.armor_penalty != 0
            || self.condition.armor_enchantment != 0
            || self.condition.hit_enchantment != 0
            || self.condition.damage_enchantment != 0
    }
}
impl Game {
    pub(crate) fn compile_equipment_alteration(
        &self,
        actor: ActorId,
        selection: &EquippedSelection,
        alteration: EquipmentAlteration,
    ) -> Result<PlannedEquipmentAlteration, GameError> {
        if !selection.accepts(alteration) || !selection.valid(&self.content.rules) {
            return Err(invalid());
        }
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let mut targets = Vec::new();
        for (socket, id) in &owner.equipment {
            let item = self.items.get(id).ok_or(GameError::ItemUnavailable)?;
            let selected = match selection {
                EquippedSelection::All => true,
                EquippedSelection::Slot { slot, class, .. } => {
                    owner.anatomy.slots.get(usize::from(socket.0)) == Some(slot)
                        && item.spec.class == *class
                }
            };
            let eligible = match alteration {
                EquipmentAlteration::EnchantArmor | EquipmentAlteration::ProtectArmor => {
                    item.spec.equipment.as_ref().and_then(|e| e.armor).is_some()
                }
                EquipmentAlteration::EnchantWeapon => item
                    .spec
                    .equipment
                    .as_ref()
                    .and_then(|e| e.attack.as_ref())
                    .is_some(),
                EquipmentAlteration::RemoveCurse => true,
            };
            if selected && eligible {
                targets.push(EquippedWitness {
                    item: *id,
                    socket: *socket,
                    condition: item.condition,
                });
            }
        }
        if let EquippedSelection::Slot { last, .. } = selection {
            if *last {
                targets = targets.into_iter().rev().take(1).collect();
            } else {
                targets.truncate(1);
            }
        }
        Ok(PlannedEquipmentAlteration {
            selection: selection.clone(),
            alteration,
            targets,
        })
    }
    /// Validate all original witnesses before any effect mutates candidate state.
    pub(crate) fn validate_equipment_alterations(
        &self,
        actor: ActorId,
        effects: &[crate::abilities::ResolvedEffect],
    ) -> Result<(), GameError> {
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        for effect in effects {
            if let crate::abilities::ResolvedEffect::AlterEquipment { alteration } = effect {
                if !alteration.plan.valid(&self.content.rules) {
                    return Err(invalid());
                }
                // A populated frozen selection must still be exactly the authored
                // selection, including first/last socket and capability. Empty plans
                // deliberately remain no-ops even if a slot was filled meanwhile.
                if !alteration.plan.targets.is_empty()
                    && self.compile_equipment_alteration(
                        actor,
                        &alteration.plan.selection,
                        alteration.plan.alteration,
                    )? != alteration.plan
                {
                    return Err(GameError::ItemUnavailable);
                }
                for target in &alteration.plan.targets {
                    if owner.equipment.get(&target.socket) != Some(&target.item)
                        || self.items.get(&target.item).is_none_or(|item| {
                            item.location != ItemLocation::Carried(actor)
                                || item.condition != target.condition
                                || !item.condition_valid()
                        })
                    {
                        return Err(GameError::ItemUnavailable);
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn alter_equipment_in_outcome(
        &mut self,
        outcome: &mut crate::lifecycle::ActorOutcome,
        alteration: &ResolvedEquipmentAlteration,
    ) -> Result<bool, GameError> {
        if !outcome.state.alive() {
            return Ok(false);
        }
        let mut changed = false;
        for target in &alteration.plan.targets {
            let item = self
                .items
                .get(&target.item)
                .ok_or(GameError::ItemUnavailable)?;
            let before = item.condition;
            let after = match alteration.plan.alteration {
                EquipmentAlteration::EnchantArmor => before.enchant_armor(
                    item.spec
                        .equipment
                        .as_ref()
                        .and_then(|e| e.armor)
                        .ok_or(GameError::ItemUnavailable)?,
                ),
                EquipmentAlteration::EnchantWeapon => {
                    if alteration.hit {
                        before.enchant_weapon_hit()
                    } else {
                        before.enchant_weapon_damage()
                    }
                }
                EquipmentAlteration::ProtectArmor => Some(before.protect()),
                EquipmentAlteration::RemoveCurse => Some(before.uncurse()),
            }
            .ok_or_else(invalid)?;
            let mut candidate = item.clone();
            candidate.condition = after;
            if !candidate.condition_valid() {
                return Err(invalid());
            }
            changed |= before != after;
            outcome
                .state
                .known_equipment
                .observe_change(target.item, before, after);
            self.items
                .edit(target.item, |item| item.condition = after)
                .ok_or(GameError::ItemUnavailable)?;
        }
        if changed {
            let sources = outcome
                .state
                .equipment
                .iter()
                .map(|(socket, id)| {
                    self.items
                        .get(id)
                        .and_then(|i| i.equipment_source(*id, *socket, &self.content.rules))
                        .ok_or(GameError::ItemUnavailable)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let context = self.equipment_recovery_context(outcome.id, &sources)?;
            if let Some(combat) = outcome.state.combat.as_mut() {
                combat
                    .set_equipment_in_context(sources, context)
                    .map_err(GameError::Creature)?;
            }
        }
        Ok(changed)
    }
}
fn invalid() -> GameError {
    GameError::Rule(crate::abilities::RuleError::InvalidPlan)
}
