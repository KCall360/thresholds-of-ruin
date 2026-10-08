//! Anatomy, ordinary equipment actions and reusable immediate effect definitions.
use crate::combat::{AttackSpec, CombatEvent, CombatSpec, DamageType, Preparation};
use crate::{Action, ActorId, Game, GameError, ItemClass, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

/// Stable index into this actor's anatomy; two rings occupy different sockets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EquipmentSlotId(pub u16);

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
}
impl AnatomySpec {
    pub fn humanoid() -> Self {
        Self {
            slots: vec![
                EquipmentSlot::Weapon,
                EquipmentSlot::Shield,
                EquipmentSlot::BodyArmor,
                EquipmentSlot::HeadArmor,
                EquipmentSlot::HandsArmor,
                EquipmentSlot::FeetArmor,
                EquipmentSlot::Cloak,
                EquipmentSlot::Ring,
                EquipmentSlot::Ring,
                EquipmentSlot::Amulet,
            ],
        }
    }
    pub fn valid(&self) -> bool {
        self.slots.len() <= 64
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSpec {
    pub slot: EquipmentSlot,
    pub attack: Option<AttackSpec>,
    pub defense: i32,
    pub reductions: BTreeMap<DamageType, u32>,
}
impl EquipmentSpec {
    pub fn valid(&self, class: ItemClass) -> bool {
        let compatible = match self.slot {
            EquipmentSlot::Weapon => class == ItemClass::Weapon,
            EquipmentSlot::Ring => class == ItemClass::Ring,
            EquipmentSlot::Amulet => class == ItemClass::Amulet,
            _ => class == ItemClass::Armor,
        };
        compatible
            && (-1000..=1000).contains(&self.defense)
            && self.reductions.values().all(|value| *value <= 1_000_000)
            && match self.slot {
                EquipmentSlot::Weapon => {
                    self.defense == 0
                        && self.reductions.is_empty()
                        && self.attack.as_ref().is_some_and(|attack| {
                            CombatSpec {
                                attack: attack.clone(),
                                ..Default::default()
                            }
                            .valid()
                        })
                }
                _ => self.attack.is_none(),
            }
    }
}

/// Shared primitives usable by items and later ability/trap sources.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectSpec {
    Heal {
        amount: u32,
    },
    Damage {
        components: BTreeMap<DamageType, u32>,
    },
}
impl EffectSpec {
    pub fn valid(&self) -> bool {
        match self {
            Self::Heal { amount } => (1..=1_000_000).contains(amount),
            Self::Damage { components } => {
                !components.is_empty()
                    && components.values().all(|value| *value <= 1_000_000)
                    && components.values().any(|value| *value > 0)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<EffectSpec>,
}

/// Controlled actor's anatomy and carried-item affordances. Equipment statistics
/// remain absent until that actor knows the item's identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionView {
    pub slots: Vec<EquipmentSlot>,
    pub preparation: Option<PreparationView>,
    pub inventory: Vec<ItemInteractionView>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparationView {
    pub work: Work,
    pub remaining: u64,
    pub active: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInteractionView {
    pub item: ItemId,
    pub slot: Option<EquipmentSlot>,
    pub equipped_slot: Option<EquipmentSlotId>,
    pub known_equipment: Option<EquipmentSpec>,
    pub drinkable: bool,
}
impl ConsumableSpec {
    pub fn valid(&self) -> bool {
        !self.effects.is_empty()
            && self.effects.len() <= 16
            && self.effects.iter().all(EffectSpec::valid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Work {
    Attack { target: ActorId },
    Equip { item: ItemId, slot: EquipmentSlotId },
    Unequip { item: ItemId },
    Drink { item: ItemId },
}
impl Work {
    pub fn action(self) -> Action {
        match self {
            Self::Attack { target } => Action::Attack { target },
            Self::Equip { item, slot } => Action::Equip { item, slot },
            Self::Unequip { item } => Action::Unequip { item },
            Self::Drink { item } => Action::Drink { item },
        }
    }
    pub fn target(self) -> Option<ActorId> {
        if let Self::Attack { target } = self {
            Some(target)
        } else {
            None
        }
    }
    pub fn item(self) -> Option<ItemId> {
        match self {
            Self::Attack { .. } => None,
            Self::Equip { item, .. } | Self::Unequip { item } | Self::Drink { item } => Some(item),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Attack { .. } => "attack",
            Self::Equip { .. } => "equip",
            Self::Unequip { .. } => "remove",
            Self::Drink { .. } => "drink",
        }
    }
    pub(crate) fn structural_valid(self, actor: ActorId) -> bool {
        match self {
            Self::Attack { target } => target.0 > 0 && target != actor,
            Self::Equip { item, slot } => item.0 > 0 && slot.0 < 64,
            Self::Unequip { item } | Self::Drink { item } => item.0 > 0,
        }
    }
}

impl Game {
    pub fn configure_anatomy(
        &mut self,
        actor: ActorId,
        anatomy: AnatomySpec,
    ) -> Result<(), GameError> {
        let mut actor = self.actors.get_mut(&actor).ok_or(GameError::UnknownActor)?;
        if !anatomy.valid() || !actor.equipment.is_empty() || actor.pending.is_some() {
            return Err(GameError::InvalidLocation);
        }
        actor.anatomy = tor_world::Shared::new(anatomy);
        Ok(())
    }
    pub fn anatomy(&self, actor: ActorId) -> Option<&AnatomySpec> {
        Some(&self.actors.get(&actor)?.anatomy)
    }
    pub fn equipment(&self, actor: ActorId) -> Option<&BTreeMap<EquipmentSlotId, ItemId>> {
        Some(&self.actors.get(&actor)?.equipment)
    }
    pub fn equip_authored(
        &mut self,
        actor: ActorId,
        item: ItemId,
        slot: EquipmentSlotId,
    ) -> Result<(), GameError> {
        self.work_duration(actor, Work::Equip { item, slot })?;
        self.actors
            .get_mut(&actor)
            .unwrap()
            .equipment
            .insert(slot, item);
        Ok(())
    }
    pub fn effective_combat(&self, actor: ActorId) -> Option<std::borrow::Cow<'_, CombatSpec>> {
        let a = self.actors.get(&actor)?;
        let base = &*a.combat.as_ref()?.spec;
        if a.equipment.is_empty() {
            return Some(std::borrow::Cow::Borrowed(base));
        }
        let mut combat = base.clone();
        for item in a.equipment.values() {
            let equipment = self.items.get(item)?.spec.equipment.as_ref()?;
            if let Some(attack) = &equipment.attack {
                combat.attack = attack.clone();
            }
            combat.defense = combat.defense.saturating_add(equipment.defense);
            for (kind, value) in &equipment.reductions {
                let total = combat.reductions.entry(*kind).or_default();
                *total = total.saturating_add(*value);
            }
        }
        Some(std::borrow::Cow::Owned(combat))
    }
    pub(crate) fn work_duration(&self, actor: ActorId, work: Work) -> Result<u64, GameError> {
        let a = self
            .actors
            .get(&actor)
            .filter(|a| a.alive())
            .ok_or(GameError::UnknownActor)?;
        if let Work::Attack { target } = work {
            return if self.attack_available(actor, target) {
                Ok(self.effective_combat(actor).unwrap().attack.wind_up)
            } else {
                Err(GameError::InvalidLocation)
            };
        }
        let item = work.item().unwrap();
        let i = self
            .items
            .get(&item)
            .filter(|i| i.location == ItemLocation::Carried(actor))
            .ok_or(GameError::ItemUnavailable)?;
        let turns = match work {
            Work::Equip { slot, .. } => {
                let equipment = i
                    .spec
                    .equipment
                    .as_ref()
                    .ok_or(GameError::ItemUnavailable)?;
                if a.anatomy.slots.get(usize::from(slot.0)) != Some(&equipment.slot)
                    || a.equipment.contains_key(&slot)
                    || a.equipment.values().any(|id| *id == item)
                    || (equipment.slot == EquipmentSlot::Weapon
                        && a.equipment.values().any(|id| {
                            self.items[id]
                                .spec
                                .equipment
                                .as_ref()
                                .is_some_and(|e| e.slot == EquipmentSlot::Weapon)
                        }))
                {
                    return Err(GameError::ItemUnavailable);
                }
                if equipment.slot == EquipmentSlot::BodyArmor {
                    3
                } else {
                    1
                }
            }
            Work::Unequip { .. } => {
                if !a.equipment.values().any(|id| *id == item) {
                    return Err(GameError::ItemUnavailable);
                }
                if i.spec
                    .equipment
                    .as_ref()
                    .ok_or(GameError::ItemUnavailable)?
                    .slot
                    == EquipmentSlot::BodyArmor
                {
                    3
                } else {
                    1
                }
            }
            Work::Drink { .. } => {
                if i.spec.consumable.is_none() || a.combat.is_none() {
                    return Err(GameError::ItemUnavailable);
                }
                1
            }
            Work::Attack { .. } => unreachable!(),
        };
        a.turn_ticks
            .get()
            .checked_mul(turns)
            .ok_or(GameError::TimeExhausted)
    }
    pub(crate) fn work_recovery(&self, actor: ActorId, work: Work) -> u64 {
        if work.target().is_some() {
            self.effective_combat(actor).unwrap().attack.recovery
        } else {
            0
        }
    }
    pub(crate) fn visible_hostiles(&self, actor: ActorId) -> BTreeSet<ActorId> {
        let Ok(scene) = self.scene(actor) else {
            return BTreeSet::new();
        };
        let cells = scene.iter().map(|cell| cell.location).collect();
        self.actors
            .perceived(&self.world, &cells)
            .into_iter()
            .filter(|(id, body)| {
                *id != actor
                    && self.actors[id].alive()
                    && self.hostile(actor, *id)
                    && body.iter().any(|(at, _)| cells.contains(at))
            })
            .map(|(id, _)| id)
            .collect()
    }
    pub(crate) fn start_work(
        &mut self,
        actor: ActorId,
        work: Work,
        intention: Option<crate::IntentionId>,
    ) {
        let duration = if work.target().is_some() {
            self.effective_combat(actor).unwrap().attack.wind_up
        } else {
            self.work_duration(actor, work).expect("validated work")
        };
        let threats = if work.item().is_some() {
            self.visible_hostiles(actor)
        } else {
            BTreeSet::new()
        };
        let mut a = self.actors.get_mut(&actor).unwrap();
        let remaining = a
            .pending
            .as_ref()
            .filter(|p| p.work == work)
            .map_or(duration, |p| p.remaining);
        a.pending = Some(Preparation {
            intention,
            work,
            threats,
            remaining,
            started: self.tick,
            active: true,
        });
    }
    pub fn apply_effects(
        &mut self,
        actor: ActorId,
        effects: &[EffectSpec],
    ) -> Result<bool, GameError> {
        if self.health(actor).is_none_or(|(hp, _)| hp == 0) {
            return Err(GameError::UnknownActor);
        }
        if effects.is_empty() || effects.len() > 16 || !effects.iter().all(EffectSpec::valid) {
            return Err(GameError::InvalidLocation);
        }
        self.next_item_id
            .checked_add(self.actors.len() as u64)
            .ok_or(GameError::IdentityExhausted)?;
        let mut observed = false;
        for effect in effects {
            if !self.alive(actor) {
                break;
            }
            observed |= match effect {
                EffectSpec::Heal { amount } => {
                    let mut a = self.actors.get_mut(&actor).unwrap();
                    let c = a.combat.as_mut().unwrap();
                    let before = c.hp;
                    c.hp = c.hp.saturating_add(*amount).min(c.spec.max_hp);
                    c.hp != before
                }
                EffectSpec::Damage { components } => self.apply_damage(actor, components) > 0,
            };
        }
        Ok(observed)
    }
    pub(crate) fn finish_item_work(
        &mut self,
        actor: ActorId,
        work: Work,
        intention: Option<crate::IntentionId>,
    ) {
        let item = work.item().unwrap();
        let event_index = self.combat.events.len();
        match work {
            Work::Equip { slot, .. } => {
                self.actors
                    .get_mut(&actor)
                    .unwrap()
                    .equipment
                    .insert(slot, item);
            }
            Work::Unequip { .. } => {
                self.actors
                    .get_mut(&actor)
                    .unwrap()
                    .equipment
                    .retain(|_, id| *id != item);
            }
            Work::Drink { .. } => {
                let spec = self.items[&item].spec.clone();
                if self.items[&item].quantity == 1 {
                    self.items.remove(&item);
                } else {
                    self.items.edit(item, |i| i.quantity -= 1).unwrap();
                }
                if self
                    .apply_effects(actor, &spec.consumable.as_ref().unwrap().effects)
                    .expect("validated effect")
                {
                    self.actors
                        .get_mut(&actor)
                        .unwrap()
                        .knowledge
                        .insert(spec.identity.clone());
                }
            }
            Work::Attack { .. } => unreachable!(),
        }
        self.combat.events.insert(
            event_index,
            CombatEvent::ItemCompleted {
                actor,
                work,
                intention,
            },
        );
    }
    pub(crate) fn work_state_valid(&self, actor: ActorId) -> bool {
        let a = &self.actors[&actor];
        let equipped: BTreeSet<_> = a.equipment.values().collect();
        a.anatomy.valid()
            && equipped.len() == a.equipment.len()
            && a.equipment.iter().all(|(slot, item)| {
                self.items.get(item).is_some_and(|i| {
                    i.location == ItemLocation::Carried(actor)
                        && i.spec.equipment.as_ref().is_some_and(|e| {
                            a.anatomy.slots.get(usize::from(slot.0)) == Some(&e.slot)
                        })
                })
            })
            && a.equipment
                .values()
                .filter(|id| {
                    self.items[id]
                        .spec
                        .equipment
                        .as_ref()
                        .is_some_and(|e| e.slot == EquipmentSlot::Weapon)
                })
                .count()
                <= 1
            && a.pending.as_ref().is_none_or(|p| {
                a.alive()
                    && p.work.structural_valid(actor)
                    && p.started <= self.tick
                    && p.started.checked_add(p.remaining).is_some()
                    && (!p.active || p.started + p.remaining >= self.actor_clock(actor))
                    && p.threats
                        .iter()
                        .all(|id| self.actors.contains_key(id) || self.detached_actor(*id))
                    && match p.work {
                        Work::Attack { target } => {
                            self.actors.contains_key(&target)
                                && self.effective_combat(actor).is_some_and(|c| {
                                    p.remaining <= c.attack.wind_up
                                        && p.started
                                            .checked_add(p.remaining)
                                            .and_then(|tick| tick.checked_add(c.attack.recovery))
                                            .is_some()
                                })
                        }
                        _ => self
                            .work_duration(actor, p.work)
                            .is_ok_and(|duration| p.remaining <= duration),
                    }
            })
    }
}
