//! Item decisions inspect definitions only after the acting actor knows them.
use crate::{
    combat::CombatSpec, Action, ActorId, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game,
    ItemId, Observation,
};
use tor_world::Location;

impl Game {
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
        let base = &owner.combat.as_ref()?.spec;
        let weapon = candidate.slot == EquipmentSlot::Weapon;
        let weapon_equipped = weapon
            && owner.equipment.values().any(|item| {
                self.items[item]
                    .spec
                    .equipment
                    .as_ref()
                    .is_some_and(|gear| gear.slot == EquipmentSlot::Weapon)
            });
        if !weapon_equipped && dominates(candidate, None, base) {
            if let Some((slot, _)) = owner.anatomy.slots.iter().enumerate().find(|(slot, kind)| {
                **kind == candidate.slot
                    && !owner.equipment.contains_key(&EquipmentSlotId(*slot as u16))
            }) {
                return Some((EquipmentSlotId(slot as u16), None));
            }
        }
        owner.equipment.iter().find_map(|(slot, item)| {
            let current = self.known_gear(actor, *item)?;
            (current.slot == candidate.slot && dominates(candidate, Some(current), base))
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
        let base = &self.actors.get(&actor)?.combat.as_ref()?.spec;
        // Recompute a nondominated choice after removal. This avoids equipping
        // the old item again without persisting a stale replacement target.
        let (item, (slot, old)) = choices.iter().find(|(item, _)| {
            let candidate = self.known_gear(actor, *item).expect("known choice");
            !choices.iter().any(|(other, _)| {
                self.known_gear(actor, *other).is_some_and(|gear| {
                    gear.slot == candidate.slot && dominates(gear, Some(candidate), base)
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

/// Strict improvement without trading away any known combat attribute.
fn dominates(new: &EquipmentSpec, old: Option<&EquipmentSpec>, base: &CombatSpec) -> bool {
    if new.slot == EquipmentSlot::Weapon {
        let Some(new) = &new.attack else {
            return false;
        };
        let old = old
            .and_then(|old| old.attack.as_ref())
            .unwrap_or(&base.attack);
        new.bonus >= old.bonus
            && new.wind_up <= old.wind_up
            && new.recovery <= old.recovery
            && old
                .damage
                .iter()
                .all(|(kind, amount)| new.damage.get(kind).copied().unwrap_or(0) >= *amount)
            && (new.bonus > old.bonus
                || new.wind_up < old.wind_up
                || new.recovery < old.recovery
                || new
                    .damage
                    .iter()
                    .any(|(kind, amount)| *amount > old.damage.get(kind).copied().unwrap_or(0)))
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
        let base = CombatSpec::default();
        let old = EquipmentSpec {
            slot: EquipmentSlot::HeadArmor,
            attack: None,
            defense: 1,
            reductions: BTreeMap::from([(DamageType::Impact, 2)]),
        };
        let mut new = old.clone();
        new.defense = 3;
        assert!(dominates(&new, Some(&old), &base));
        new.reductions.clear();
        assert!(!dominates(&new, Some(&old), &base));
        let old = EquipmentSpec {
            slot: EquipmentSlot::Weapon,
            attack: Some(base.attack.clone()),
            defense: 0,
            reductions: BTreeMap::new(),
        };
        let mut new = old.clone();
        new.attack.as_mut().unwrap().bonus += 1;
        assert!(dominates(&new, Some(&old), &base));
        new.attack.as_mut().unwrap().wind_up += 1;
        assert!(!dominates(&new, Some(&old), &base));
        new.attack.as_mut().unwrap().wind_up = base.attack.wind_up;
        new.attack.as_mut().unwrap().damage.clear();
        assert!(!dominates(&new, Some(&old), &base));
    }
}
