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
    /// Compile per-instance signed potency into ordinary source-owned operands.
    /// Only numeric operations are scalable: boolean grants never gain accidental
    /// inverse or duplicate semantics from a cursed instance.
    pub(crate) fn scaled_potency_modifiers(&self, potency: i32) -> Option<Vec<Modifier>> {
        if !(-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(potency)) {
            return None;
        }
        let mut modifiers = self.potency_modifiers.clone();
        for modifier in &mut modifiers {
            match &mut modifier.op {
                ModifierOp::Add { value, .. }
                | ModifierOp::AttributeWithoutDefense { value, .. } => {
                    *value = value.checked_mul(i64::from(potency))?;
                }
                ModifierOp::SignedReduction { amount, .. } => {
                    *amount = amount.checked_mul(potency)?;
                }
                _ => return None,
            }
        }
        crate::rules::validate_modifier_shapes(&modifiers).ok()?;
        Some(modifiers)
    }
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ItemCondition {
    pub corrosion: u32,
    pub cursed: bool,
    pub protected: bool,
    pub armor_penalty: u32,
    pub armor_enchantment: u32,
    pub hit_enchantment: i32,
    pub damage_enchantment: i32,
    pub potency: i32,
}
impl<'de> Deserialize<'de> for ItemCondition {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            corrosion: u32,
            cursed: bool,
            protected: bool,
            armor_penalty: u32,
            armor_enchantment: u32,
            hit_enchantment: i32,
            damage_enchantment: i32,
            potency: i32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let condition = Self {
            corrosion: fields.corrosion,
            cursed: fields.cursed,
            protected: fields.protected,
            armor_penalty: fields.armor_penalty,
            armor_enchantment: fields.armor_enchantment,
            hit_enchantment: fields.hit_enchantment,
            damage_enchantment: fields.damage_enchantment,
            potency: fields.potency,
        };
        if !condition.valid() {
            return Err(serde::de::Error::custom("invalid item condition"));
        }
        Ok(condition)
    }
}
impl ItemCondition {
    /// Validate each authored weapon payload independently. A dual-capability
    /// item may not hide an invalid attack behind its other delivery mode.
    pub(crate) fn weapon_capabilities_valid(
        self,
        equipment: Option<&crate::EquipmentSpec>,
        projectile: Option<&crate::projectile_profiles::ProjectileSpec>,
    ) -> bool {
        let attack = equipment.and_then(|gear| gear.attack.as_ref());
        (attack.is_some()
            || projectile.is_some()
            || (self.hit_enchantment == 0 && self.damage_enchantment == 0))
            && attack.is_none_or(|attack| {
                attack
                    .with_enchantment(self.hit_enchantment, self.damage_enchantment)
                    .is_ok()
            })
            && projectile.is_none_or(|projectile| {
                projectile.validate_shape().is_ok()
                    && projectile
                        .damage
                        .with_melee_modifiers(0, self.damage_enchantment, 0)
                        .is_ok()
            })
    }
    pub(crate) fn valid(self) -> bool {
        self.corrosion <= crate::limits::NUMERIC as u32
            && self.armor_penalty <= crate::limits::NUMERIC as u32
            && self.armor_enchantment <= crate::limits::NUMERIC as u32
            && [self.hit_enchantment, self.damage_enchantment, self.potency]
                .into_iter()
                .all(|value| {
                    (-crate::limits::NUMERIC..=crate::limits::NUMERIC).contains(&i64::from(value))
                })
    }

    /// Rust and negative physical condition affect only mundane plating.
    /// Both losses retain their exact history; overlapping losses floor at zero.
    pub(crate) fn physical_armor(self, armor: ArmorSpec) -> Option<u32> {
        if !self.valid()
            || !armor.valid()
            || self.armor_penalty > armor.physical_reduction
            || self.corrosion > armor.physical_reduction
            || (self.corrosion != 0 && !armor.rustable())
        {
            return None;
        }
        let amount = i64::from(armor.physical_reduction)
            .checked_sub(i64::from(self.armor_penalty))?
            .checked_sub(i64::from(self.corrosion))?
            .max(0);
        Some(amount as u32)
    }

    /// Total protection against physical damage includes the five-category ward.
    pub(crate) fn effective_armor(self, armor: ArmorSpec) -> Option<u32> {
        let amount = self
            .physical_armor(armor)?
            .checked_add(self.armor_enchantment)?;
        (amount <= crate::limits::NUMERIC as u32).then_some(amount)
    }

    /// Each enchantment adds one ward point against the five named ward types.
    /// Psychic damage is excluded.
    /// Clearing the curse does not repair physical penalty or corrosion.
    pub(crate) fn enchant_armor(self, armor: ArmorSpec) -> Option<Self> {
        self.effective_armor(armor)?;
        let changed = Self {
            armor_enchantment: self.armor_enchantment.checked_add(1)?,
            cursed: false,
            ..self
        };
        changed.effective_armor(armor)?;
        Some(changed)
    }

    pub(crate) fn enchant_weapon_hit(self) -> Option<Self> {
        let changed = Self {
            hit_enchantment: self.hit_enchantment.checked_add(1)?,
            cursed: false,
            ..self
        };
        (self.valid() && changed.valid()).then_some(changed)
    }

    pub(crate) fn enchant_weapon_damage(self) -> Option<Self> {
        let changed = Self {
            damage_enchantment: self.damage_enchantment.checked_add(1)?,
            cursed: false,
            ..self
        };
        (self.valid() && changed.valid()).then_some(changed)
    }

    pub(crate) fn uncurse(self) -> Self {
        Self {
            cursed: false,
            ..self
        }
    }

    pub(crate) fn protect(self) -> Self {
        Self {
            protected: true,
            ..self
        }
    }

    pub(crate) fn corrode_armor(self, armor: ArmorSpec, requested: u32) -> Option<(Self, u32)> {
        self.effective_armor(armor)?;
        let effective = self.physical_armor(armor)?;
        if requested == 0 || requested > crate::limits::NUMERIC as u32 {
            return None;
        }
        if self.protected || !armor.rustable() {
            return Some((self, 0));
        }
        let amount = requested.min(effective);
        let changed = Self {
            corrosion: self.corrosion.checked_add(amount)?,
            ..self
        };
        changed.effective_armor(armor)?;
        Some((changed, amount))
    }
}
impl Item {
    pub(crate) fn condition_valid(&self) -> bool {
        if !self.condition.valid() {
            return false;
        }
        let gear = self.spec.equipment.as_ref();
        let armor_valid = match gear.and_then(|gear| gear.armor) {
            Some(armor) => self.condition.effective_armor(armor).is_some(),
            None => {
                self.condition.corrosion == 0
                    && self.condition.armor_penalty == 0
                    && self.condition.armor_enchantment == 0
                    && !self.condition.protected
            }
        };
        armor_valid
            && (self.condition.potency == 0
                || gear.is_some_and(|g| !g.potency_modifiers.is_empty()))
            && gear.is_none_or(|g| g.scaled_potency_modifiers(self.condition.potency).is_some())
            && self
                .condition
                .weapon_capabilities_valid(gear, self.spec.projectile.as_ref())
            && (self.spec.has_instance_condition() || !self.condition.cursed)
    }

    /// Identified equipment shows its current contribution, not unworn catalog values.
    pub(crate) fn equipment_view(&self) -> Option<crate::EquipmentSpec> {
        if !self.condition_valid() {
            return None;
        }
        let mut equipment = self.spec.equipment.clone()?;
        if let Some(armor) = equipment.armor.as_mut() {
            armor.physical_reduction = self.condition.effective_armor(*armor)?;
        }
        equipment.modifiers = equipment.modifiers_with_armor(0)?;
        equipment
            .modifiers
            .extend(equipment.scaled_potency_modifiers(self.condition.potency)?);
        equipment.potency_modifiers.clear();
        if equipment.armor.is_some() && self.condition.armor_enchantment != 0 {
            for category in [DamageType::Energy, DamageType::Spirit, DamageType::Vital] {
                equipment.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Reduction {
                        selector: Selector::Category { category },
                        amount: self.condition.armor_enchantment,
                    },
                });
            }
        }
        equipment.armor = None;
        if let Some(attack) = equipment.attack.as_ref() {
            equipment.attack = Some(
                attack
                    .with_enchantment(
                        self.condition.hit_enchantment,
                        self.condition.damage_enchantment,
                    )
                    .ok()?,
            );
        }
        Some(equipment)
    }

    /// One contribution path for equip, previews, restoration and corrosion.
    /// The permanent compiled creature is retained by the ordinary equipment fold.
    pub(crate) fn equipment_source(
        &self,
        id: ItemId,
        socket: EquipmentSlotId,
        rules: &crate::rules::Ruleset,
    ) -> Option<crate::creatures::EquipmentSource> {
        if !self.condition_valid() {
            return None;
        }
        let equipment = self.equipment_view()?;
        Some(crate::creatures::EquipmentSource {
            item: id,
            socket,
            weapon_profile: equipment
                .weapon_profile
                .as_ref()
                .map(|profile| profile.validate(rules))
                .transpose()
                .ok()?,
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
        if !outcome.state.alive()
            || outcome
                .state
                .combat
                .as_ref()
                .is_some_and(|combat| combat.creature().derived().protects_armor)
        {
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
        let (condition, amount) =
            expected
                .condition
                .corrode_armor(armor, requested)
                .ok_or(crate::GameError::Rule(
                    crate::abilities::RuleError::InvalidPlan,
                ))?;
        if amount == 0 {
            return Ok(0);
        }
        let mut changed = expected.clone();
        changed.condition = condition;
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
                item.equipment_source(*id, *socket, &self.content.rules)
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
        outcome.state.known_equipment.observe_change(
            item_id,
            expected.condition,
            changed.condition,
        );
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
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
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
            relocation_elapsed: None,
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
    fn uncurse_preserves_all_other_condition_operands() {
        let condition = ItemCondition {
            corrosion: 4,
            cursed: true,
            protected: true,
            armor_penalty: 2,
            armor_enchantment: 3,
            hit_enchantment: -2,
            damage_enchantment: 1,
            potency: -1,
        };
        assert_eq!(
            condition.uncurse(),
            ItemCondition {
                cursed: false,
                ..condition
            }
        );
        assert_eq!(condition.uncurse().uncurse(), condition.uncurse());
    }

    #[test]
    fn weapon_adjustments_remain_signed_until_final_damage_fold() {
        let base = crate::attacks::MeleeAttack::fixed(
            crate::attributes::Skill::LightWeaponry,
            -2,
            1,
            1,
            DamageType::Keen,
            None,
            2,
        )
        .unwrap();
        let adjusted = base.with_enchantment(-3, -5).unwrap();
        assert_eq!(adjusted.bonus(), -5);
        assert_eq!(adjusted.damage(), base.damage());
        let damage = adjusted.damage_with_modifiers(0, 4, 0).unwrap();
        assert_eq!(
            damage.components()[0].amount(),
            crate::damage::DamageAmount::Fixed(1)
        );
        let selected = crate::attacks::SelectedMeleeAttack::Weapon(adjusted.clone());
        assert_eq!(selected.damage_with_modifiers(0, 4, 0).unwrap(), damage);
        let round_trip: crate::attacks::MeleeAttack =
            serde_json::from_value(serde_json::to_value(&adjusted).unwrap()).unwrap();
        assert_eq!(round_trip, adjusted);
        assert!(crate::attacks::NaturalAttack::from_melee(adjusted.clone()).is_err());
        let mut raw = serde_json::to_value(&adjusted).unwrap();
        raw.as_object_mut().unwrap().remove("damage_enchantment");
        assert!(serde_json::from_value::<crate::attacks::MeleeAttack>(raw).is_err());
        let mut weapon = item(metal());
        let mut spec = (*weapon.spec).clone();
        spec.equipment.as_mut().unwrap().attack = Some(base.clone());
        weapon.spec = tor_world::Shared::new(spec);
        let sibling = weapon.clone();
        weapon.condition.hit_enchantment = -3;
        weapon.condition.damage_enchantment = -5;
        assert!(weapon.condition_valid());
        let view = weapon.equipment_view().unwrap();
        let source = weapon
            .equipment_source(
                ItemId(1),
                EquipmentSlotId(0),
                &crate::test_creatures::ruleset(),
            )
            .unwrap();
        assert_eq!(view.attack, source.attack);
        assert_eq!(view.attack, Some(adjusted));
        assert_eq!(weapon.spec, sibling.spec);
        assert_eq!(sibling.equipment_view().unwrap().attack, Some(base.clone()));
        let hit = weapon.condition.enchant_weapon_hit().unwrap();
        assert_eq!(hit.hit_enchantment, -2);
        assert_eq!(hit.damage_enchantment, -5);
        let damage = hit.enchant_weapon_damage().unwrap();
        assert_eq!(damage.hit_enchantment, -2);
        assert_eq!(damage.damage_enchantment, -4);
        assert!(base.with_enchantment(i32::MAX, 0).is_err());
        assert!(base.with_enchantment(0, i32::MIN).is_err());
    }

    fn reductions(owned: &Item) -> [u32; 6] {
        let mut result = [0; 6];
        for modifier in owned.equipment_view().unwrap().modifiers {
            if let ModifierOp::Reduction {
                selector: Selector::Category { category },
                amount,
            } = modifier.op
            {
                let index = match category {
                    DamageType::Impact => 0,
                    DamageType::Keen => 1,
                    DamageType::Energy => 2,
                    DamageType::Spirit => 3,
                    DamageType::Vital => 4,
                    DamageType::Psychic => 5,
                };
                result[index] += amount;
            }
        }
        result
    }

    #[test]
    fn armor_ward_adds_one_to_five_categories_except_psychic_without_repairing_penalty() {
        let mut owned = item(metal());
        owned.condition = ItemCondition {
            armor_penalty: 2,
            cursed: true,
            ..Default::default()
        };
        assert_eq!(reductions(&owned), [0, 0, 7, 0, 0, 0]);
        for level in 1..=3 {
            owned.condition = owned.condition.enchant_armor(metal()).unwrap();
            assert_eq!(owned.condition.armor_enchantment, level);
            assert_eq!(owned.condition.armor_penalty, 2);
            assert_eq!(owned.condition.corrosion, 0);
            assert!(!owned.condition.cursed);
            assert_eq!(
                reductions(&owned),
                [level, level, 7 + level, level, level, 0]
            );
        }
        assert!(ItemCondition {
            armor_penalty: 3,
            ..Default::default()
        }
        .effective_armor(metal())
        .is_none());
    }

    #[test]
    fn corrosion_consumes_only_physical_armor_and_protection_preserves_history() {
        let condition = ItemCondition {
            armor_enchantment: 3,
            cursed: true,
            ..Default::default()
        };
        let (damaged, amount) = condition.corrode_armor(metal(), 5).unwrap();
        assert_eq!(amount, 2);
        assert_eq!(damaged.corrosion, 2);
        assert_eq!(damaged.effective_armor(metal()), Some(3));
        assert_eq!(damaged.corrode_armor(metal(), 1), Some((damaged, 0)));
        let mut owned = item(metal());
        owned.condition = damaged;
        assert_eq!(reductions(&owned), [3, 3, 10, 3, 3, 0]);
        let enchanted = damaged.enchant_armor(metal()).unwrap();
        assert_eq!(enchanted.armor_enchantment, 4);
        assert_eq!(enchanted.corrosion, 2);
        assert_eq!(enchanted.effective_armor(metal()), Some(4));
        let damaged = ItemCondition {
            corrosion: 1,
            armor_penalty: 1,
            ..condition
        };
        let plating = ArmorSpec {
            physical_reduction: 3,
            ..metal()
        };
        let protected = damaged.protect();
        assert!(protected.cursed);
        assert_eq!(protected.corrosion, 1);
        assert_eq!(protected.armor_penalty, 1);
        assert_eq!(protected.effective_armor(plating), Some(4));
        assert_eq!(protected.corrode_armor(plating, 3), Some((protected, 0)));
        assert_eq!(damaged.corrode_armor(plating, 3).unwrap().1, 1);
        let uncursed = protected.uncurse();
        assert!(!uncursed.cursed);
        assert!(uncursed.protected);
        assert_eq!(uncursed.corrosion, 1);
        assert_eq!(uncursed.armor_penalty, 1);
        let mut owned = item(plating);
        owned.condition = protected;
        assert!(owned.condition_valid());
    }

    #[test]
    fn enchantment_bounds_fail_without_clearing_curse_or_mutating_siblings() {
        let original = item(metal());
        let mut changed = original.clone();
        changed.condition = changed.condition.enchant_armor(metal()).unwrap();
        assert_eq!(changed.spec, original.spec);
        assert_eq!(original.condition, ItemCondition::default());
        assert_ne!(changed.equipment_view(), original.equipment_view());
        let bound = ItemCondition {
            armor_enchantment: crate::limits::NUMERIC as u32 - 2,
            cursed: true,
            ..Default::default()
        };
        assert_eq!(
            bound.effective_armor(metal()),
            Some(crate::limits::NUMERIC as u32)
        );
        assert!(bound.enchant_armor(metal()).is_none());
        assert!(bound.cursed);
        assert!(ItemCondition {
            hit_enchantment: i32::MAX,
            ..Default::default()
        }
        .enchant_weapon_hit()
        .is_none());
        assert!(ItemCondition {
            damage_enchantment: crate::limits::NUMERIC as i32,
            ..Default::default()
        }
        .enchant_weapon_damage()
        .is_none());
        assert!(ItemCondition {
            corrosion: u32::MAX,
            ..Default::default()
        }
        .effective_armor(metal())
        .is_none());
    }

    #[test]
    fn item_owned_corrosion_changes_only_its_physical_reduction() {
        let original = item(metal());
        let mut worn = original.clone();
        for loss in 0..=2 {
            worn.condition.corrosion = loss;
            let source = worn
                .equipment_source(
                    ItemId(1),
                    EquipmentSlotId(0),
                    &crate::test_creatures::ruleset(),
                )
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
            .equipment_source(
                ItemId(1),
                EquipmentSlotId(0),
                &crate::test_creatures::ruleset()
            )
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
        let raw = serde_json::to_value(ItemCondition::default()).unwrap();
        assert_eq!(
            serde_json::from_value::<ItemCondition>(raw.clone()).unwrap(),
            ItemCondition::default()
        );
        for field in [
            "corrosion",
            "cursed",
            "protected",
            "armor_penalty",
            "armor_enchantment",
            "hit_enchantment",
            "damage_enchantment",
            "potency",
        ] {
            let mut missing = raw.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ItemCondition>(missing).is_err(),
                "{field}"
            );
        }
        for field in [
            "corrosion",
            "armor_penalty",
            "armor_enchantment",
            "hit_enchantment",
            "damage_enchantment",
            "potency",
        ] {
            let mut invalid = raw.clone();
            invalid[field] = serde_json::json!(crate::limits::NUMERIC + 1);
            assert!(
                serde_json::from_value::<ItemCondition>(invalid).is_err(),
                "{field}"
            );
        }
        for field in [
            "armor_penalty",
            "armor_enchantment",
            "hit_enchantment",
            "damage_enchantment",
            "potency",
        ] {
            let mut invalid = raw.clone();
            invalid[field] = serde_json::json!(-crate::limits::NUMERIC - 1);
            assert!(
                serde_json::from_value::<ItemCondition>(invalid).is_err(),
                "{field}"
            );
        }
        let mut unknown = raw.clone();
        unknown["extra"] = serde_json::json!(0);
        assert!(serde_json::from_value::<ItemCondition>(unknown).is_err());
        assert!(serde_json::from_value::<ItemCondition>(serde_json::json!([
            0, false, false, 0, 0, 0, 0
        ]))
        .is_err());
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
    #[test]
    fn potency_and_armor_share_the_source_modifier_budget() {
        let mut worn = item(metal());
        let mut spec = (*worn.spec).clone();
        let gear = spec.equipment.as_mut().unwrap();
        let zero = Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: crate::rules::Stat::AttackDamageRollBonus,
                value: 0,
            },
        };
        gear.modifiers = vec![zero.clone(); crate::limits::MODIFIERS_PER_SOURCE - 2];
        gear.potency_modifiers = vec![zero];
        assert!(
            !gear.valid_shape(),
            "armor adds two modifiers to the combined ordinary and potency source"
        );
        assert!(gear
            .validate_rules(crate::ItemClass::Armor, &crate::test_creatures::ruleset())
            .is_err());
        gear.modifiers.pop();
        assert!(gear.valid_shape());
        gear.validate_rules(crate::ItemClass::Armor, &crate::test_creatures::ruleset())
            .unwrap();
        worn.spec = tor_world::Shared::new(spec);
        let source = worn
            .equipment_source(
                ItemId(1),
                EquipmentSlotId(0),
                &crate::test_creatures::ruleset(),
            )
            .unwrap();
        assert_eq!(source.modifiers.len(), crate::limits::MODIFIERS_PER_SOURCE);
        crate::test_creatures::ruleset()
            .validate_modifiers(&source.modifiers)
            .unwrap();
    }
}
