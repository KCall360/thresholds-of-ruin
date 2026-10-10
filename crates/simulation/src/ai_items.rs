//! Item decisions inspect definitions only after the acting actor knows them.
use crate::{
    Action, ActorId, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game, ItemId, Observation,
    TravelStep,
};
use tor_world::Location;

impl Game {
    pub(crate) fn choose_nearby_loot(
        &self,
        actor: ActorId,
        view: &Observation,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        let has_healing = view
            .inventory
            .iter()
            .any(|item| self.known_healing(actor, item.id));
        view.ground_items
            .iter()
            .filter(|item| {
                self.gear_destination(actor, item.id).is_some()
                    || (!has_healing && self.known_healing(actor, item.id))
            })
            .filter(|item| !self.exposed_to_visible_hostile(actor, item.location, view))
            .filter_map(|item| {
                let path = route(item.location)?;
                if path.len() > 3 {
                    return None;
                }
                let action = if let Some(step) = path.first() {
                    let (at, _) = self.actor_translation(actor, step.direction)?;
                    if self.exposed_to_visible_hostile(actor, at, view) {
                        return None;
                    }
                    Action::Move(step.direction)
                } else {
                    Action::Take {
                        item: item.id,
                        quantity: Some(1),
                    }
                };
                Some((path.len(), item.id, action))
            })
            .min_by_key(|(distance, item, _)| (*distance, *item))
            .map(|(_, _, action)| action)
    }

    fn known_gear(&self, actor: ActorId, item: ItemId) -> Option<&EquipmentSpec> {
        let item = self.items.get(&item)?;
        let actor = self.actors.get(&actor)?;
        if item.spec.concealed && !actor.knowledge.contains(&item.spec.identity) {
            return None;
        }
        item.spec.equipment.as_ref()
    }

    pub(crate) fn exposed_to_visible_hostile(
        &self,
        actor: ActorId,
        at: Location,
        view: &Observation,
    ) -> bool {
        view.visible_actors
            .iter()
            .filter(|other| self.hostile(actor, other.id))
            .any(|other| {
                other.location == at
                    || (-1..=1).any(|x| {
                        (-1..=1).any(|y| {
                            (-1..=1).any(|z| {
                                [x, y, z] != [0, 0, 0]
                                    && self.melee_neighbor(other.location, [x, y, z]) == Some(at)
                            })
                        })
                    })
            })
    }

    pub(crate) fn gear_destination(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<(EquipmentSlotId, Option<ItemId>)> {
        let candidate = self.known_gear(actor, item)?;
        let owner = self.actors.get(&actor)?;
        if owner.equipment.values().any(|equipped| *equipped == item) {
            return None;
        }
        let creature = owner.combat.as_ref()?.creature();
        let weapon = candidate.slot == EquipmentSlot::Weapon;
        let weapon_equipped = weapon
            && owner.equipment.values().any(|item| {
                self.items[item]
                    .spec
                    .equipment
                    .as_ref()
                    .is_some_and(|gear| gear.slot == EquipmentSlot::Weapon)
            });
        if !weapon_equipped && dominates(candidate, None, creature) {
            if let Some((slot, _)) = owner.anatomy.slots.iter().enumerate().find(|(slot, kind)| {
                **kind == candidate.slot
                    && !owner.equipment.contains_key(&EquipmentSlotId(*slot as u16))
            }) {
                return Some((EquipmentSlotId(slot as u16), None));
            }
        }
        owner.equipment.iter().find_map(|(slot, item)| {
            let current = self.known_gear(actor, *item)?;
            (current.slot == candidate.slot && dominates(candidate, Some(current), creature))
                .then_some((*slot, Some(*item)))
        })
    }

    pub(crate) fn choose_gear(&self, actor: ActorId, view: &Observation) -> Option<Action> {
        let choices: Vec<_> = view
            .inventory
            .iter()
            .filter_map(|item| {
                self.gear_destination(actor, item.id)
                    .map(|destination| (item.id, destination))
            })
            .collect();
        let creature = self.creature(actor)?;
        // Recompute a nondominated choice after removal. This avoids equipping
        // the old item again without persisting a stale replacement target.
        let (item, (slot, old)) = choices.iter().find(|(item, _)| {
            let candidate = self.known_gear(actor, *item).expect("known choice");
            !choices.iter().any(|(other, _)| {
                self.known_gear(actor, *other).is_some_and(|gear| {
                    gear.slot == candidate.slot && dominates(gear, Some(candidate), creature)
                })
            })
        })?;
        Some(old.map_or(
            Action::Equip {
                item: *item,
                slot: *slot,
            },
            |item| Action::Unequip { item },
        ))
    }
}

/// Upgrade without lowering known check score, damage estimate or timing.
/// Rolled damage uses twice the clipped mean as an integer heuristic, not a
/// promise about individual rolls. Descriptor groups are compared separately.
fn dominates(
    new: &EquipmentSpec,
    old: Option<&EquipmentSpec>,
    creature: &crate::creatures::CreatureState,
) -> bool {
    if new.slot == EquipmentSlot::Weapon {
        let Some(new_attack) = &new.attack else {
            return false;
        };
        let score = |attack: &crate::attacks::MeleeAttack| {
            let damage = creature.derived().damage_for_melee(attack, 0).ok()?;
            let mut estimates = std::collections::BTreeMap::new();
            for component in damage.components() {
                let estimate = match component.amount() {
                    crate::damage::DamageAmount::Fixed(value) => i64::from(value) * 2,
                    crate::damage::DamageAmount::Rolled(pool) => (i64::from(pool.count())
                        * (i64::from(pool.sides()) + 1)
                        + i64::from(pool.bonus()) * 2)
                        .max(0),
                };
                let key = component.key();
                *estimates.entry((key.category, key.descriptor)).or_insert(0) += estimate;
            }
            let bonus = attack.bonus()
                + i32::from(
                    creature
                        .derived()
                        .attributes
                        .get(attack.skill().attribute(creature.build().binding())),
                )
                + i32::from(creature.derived().skills.get(attack.skill()));
            let phase = |duration| creature.derived().attributes.physical_duration(duration);
            Some((
                bonus,
                phase(attack.wind_up()),
                phase(attack.recovery()),
                estimates,
            ))
        };
        let Some((new_bonus, new_wind, new_recovery, new_damage)) = score(new_attack) else {
            return false;
        };
        let old_attack = old
            .and_then(|old| old.attack.as_ref())
            .unwrap_or(&creature.derived().melee);
        let old_score = score(old_attack);
        let Some((old_bonus, old_wind, old_recovery, old_damage)) = old_score else {
            return false;
        };
        new_bonus >= old_bonus
            && new_wind <= old_wind
            && new_recovery <= old_recovery
            && old_damage
                .iter()
                .all(|(key, value)| new_damage.get(key).copied().unwrap_or(0) >= *value)
            && (new_bonus > old_bonus
                || new_wind < old_wind
                || new_recovery < old_recovery
                || new_damage
                    .iter()
                    .any(|(key, value)| *value > old_damage.get(key).copied().unwrap_or(0)))
    } else {
        let defense = old.map_or(0, |gear| gear.defense);
        new.defense >= defense
            && old.is_none_or(|old| {
                old.reductions
                    .iter()
                    .all(|(kind, amount)| new.reductions.get(kind).copied().unwrap_or(0) >= *amount)
            })
            && (new.defense > defense
                || new.reductions.iter().any(|(kind, amount)| {
                    *amount
                        > old
                            .and_then(|old| old.reductions.get(kind))
                            .copied()
                            .unwrap_or(0)
                }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::DamageType;
    use std::collections::BTreeMap;

    #[test]
    fn upgrades_do_not_trade_away_typed_protection_or_weapon_speed() {
        let creature = crate::creatures::CreatureState::new(crate::test_creatures::build(
            crate::test_creatures::species(),
        ))
        .unwrap();
        let base = &creature.derived().melee;
        let old = EquipmentSpec {
            slot: EquipmentSlot::HeadArmor,
            attack: None,
            defense: 1,
            reductions: BTreeMap::from([(DamageType::Impact, 2)]),
        };
        let mut new = old.clone();
        new.defense = 3;
        assert!(dominates(&new, Some(&old), &creature));
        new.reductions.clear();
        assert!(!dominates(&new, Some(&old), &creature));
        let old = EquipmentSpec {
            slot: EquipmentSlot::Weapon,
            attack: Some(
                crate::attacks::MeleeAttack::fixed(
                    crate::attributes::Skill::HeavyWeaponry,
                    base.bonus(),
                    base.wind_up(),
                    base.recovery(),
                    DamageType::Impact,
                    None,
                    1,
                )
                .unwrap(),
            ),
            defense: 0,
            reductions: BTreeMap::new(),
        };
        let mut new = old.clone();
        new.attack = Some(
            crate::attacks::MeleeAttack::fixed(
                crate::attributes::Skill::HeavyWeaponry,
                base.bonus() + 1,
                base.wind_up(),
                base.recovery(),
                DamageType::Impact,
                None,
                1,
            )
            .unwrap(),
        );
        assert!(dominates(&new, Some(&old), &creature));
        new.attack = Some(
            crate::attacks::MeleeAttack::fixed(
                crate::attributes::Skill::HeavyWeaponry,
                base.bonus() + 1,
                base.wind_up() + 1,
                base.recovery(),
                DamageType::Impact,
                None,
                1,
            )
            .unwrap(),
        );
        assert!(!dominates(&new, Some(&old), &creature));
        new.attack = Some(
            crate::attacks::MeleeAttack::fixed(
                crate::attributes::Skill::HeavyWeaponry,
                base.bonus() + 1,
                base.wind_up(),
                base.recovery(),
                DamageType::Impact,
                None,
                0,
            )
            .unwrap(),
        );
        assert!(!dominates(&new, Some(&old), &creature));
    }

    #[test]
    fn weapon_choice_uses_owned_skill_and_preserves_damage_descriptors() {
        use crate::attacks::MeleeAttack;
        use crate::attributes::{Attributes, ManaBinding, Skill};
        use crate::creatures::{CreatureBuild, CreatureState, Species};
        use crate::damage::{DamageComponent, DamageSpec};
        use crate::dice::DicePool;
        use crate::grants::Descriptor;
        use crate::progression::{Class, CreatureType, HdLedger, HdSource};
        let weapon = |skill, bonus, descriptor, count| {
            let component = DamageComponent::rolled(
                DamageType::Energy,
                descriptor,
                DicePool::new(count, 6, 0).unwrap(),
            );
            let primary = component.key();
            EquipmentSpec {
                slot: EquipmentSlot::Weapon,
                attack: Some(
                    MeleeAttack::new(
                        skill,
                        bonus,
                        60,
                        40,
                        DamageSpec::new(vec![component], Some(primary)).unwrap(),
                    )
                    .unwrap(),
                ),
                defense: 0,
                reductions: BTreeMap::new(),
            }
        };
        let old = weapon(Skill::HeavyWeaponry, 0, Some(Descriptor::Fire), 1);
        let mut build = CreatureBuild::new(
            Species {
                id: "weapon_judge".into(),
                kind: CreatureType::Humanoid,
                subtypes: Default::default(),
                anatomy: crate::AnatomySpec::humanoid(),
                default_attributes: Attributes::new([5, 1, 1, 1, 1, 1]).unwrap(),
                melee: old.attack.clone().unwrap(),
                grants: vec![],
            },
            HdLedger::seeded(vec![HdSource::Class(Class::Warrior)], 42).unwrap(),
            ManaBinding::Intellect,
        )
        .unwrap();
        build.train(0, Skill::HeavyWeaponry).unwrap();
        let creature = CreatureState::new(build).unwrap();
        let before = creature.clone();
        // Better nominal weapon bonus does not compensate for losing owned training/Strength.
        let light = weapon(Skill::LightWeaponry, 1, Some(Descriptor::Fire), 2);
        assert!(!dominates(&light, Some(&old), &creature));
        // More fire dice is an upgrade; replacing fire with cold is a tradeoff.
        let fire = weapon(Skill::HeavyWeaponry, 0, Some(Descriptor::Fire), 2);
        assert!(dominates(&fire, Some(&old), &creature));
        assert!(dominates(&fire, None, &creature));
        let cold = weapon(Skill::HeavyWeaponry, 0, Some(Descriptor::Cold), 3);
        assert!(!dominates(&cold, Some(&old), &creature));
        assert_eq!(
            creature, before,
            "equipment scoring cannot alter owned source state"
        );
    }
}
