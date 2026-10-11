//! Anatomy, ordinary equipment actions and reusable immediate effect definitions.
use crate::combat::{CombatEvent, Preparation};
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
    pub attack: Option<crate::attacks::MeleeAttack>,
    pub modifiers: Vec<crate::rules::Modifier>,
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
            && crate::rules::validate_modifier_shapes(&self.modifiers).is_ok()
            && match self.slot {
                EquipmentSlot::Weapon => self.attack.is_some(),
                _ => self.attack.is_none(),
            }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<crate::rules::Effect>,
}

/// Controlled actor's anatomy and carried-item affordances. Equipment statistics
/// remain absent until that actor knows the item's identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionView {
    pub completed: Vec<Work>,
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
            && self.effects.iter().all(crate::rules::Effect::valid_shape)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Work {
    UseAbility {
        id: crate::content_ids::AbilityId,
        target: ActorId,
    },
    Equip {
        item: ItemId,
        slot: EquipmentSlotId,
    },
    Unequip {
        item: ItemId,
    },
    Drink {
        item: ItemId,
    },
}
impl Work {
    pub fn action(&self) -> Action {
        match self {
            Self::UseAbility { id, target } => Action::UseAbility {
                id: id.clone(),
                target: *target,
            },
            Self::Equip { item, slot } => Action::Equip {
                item: *item,
                slot: *slot,
            },
            Self::Unequip { item } => Action::Unequip { item: *item },
            Self::Drink { item } => Action::Drink { item: *item },
        }
    }
    pub fn target(&self) -> Option<ActorId> {
        match self {
            Self::UseAbility { target, .. } => Some(*target),
            Self::Equip { .. } | Self::Unequip { .. } | Self::Drink { .. } => None,
        }
    }
    pub fn item(&self) -> Option<ItemId> {
        match self {
            Self::UseAbility { .. } => None,
            Self::Equip { item, .. } | Self::Unequip { item } | Self::Drink { item } => Some(*item),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::UseAbility { .. } => "use ability",
            Self::Equip { .. } => "equip",
            Self::Unequip { .. } => "remove",
            Self::Drink { .. } => "drink",
        }
    }
    pub(crate) fn structural_valid(&self, _actor: ActorId) -> bool {
        match self {
            Self::UseAbility { target, .. } => target.0 > 0,
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
        if !anatomy.valid()
            || !actor.equipment.is_empty()
            || actor.pending.is_some()
            || actor
                .combat
                .as_ref()
                .map(|combat| combat.creature())
                .is_some_and(|creature| creature.derived().anatomy != anatomy)
        {
            return Err(GameError::Build(
                crate::creatures::BuildError::InvalidDefinition,
            ));
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
        self.item_work_duration(actor, Work::Equip { item, slot })?;
        let mut equipment = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .equipment
            .clone();
        equipment.insert(slot, item);
        self.set_actor_equipment(actor, equipment)
    }

    fn equipment_candidate(
        &self,
        actor: ActorId,
        equipment: &BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<Option<EquippedCreature>, GameError> {
        let Some(mut combat) = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .combat
            .clone()
        else {
            return Ok(None);
        };
        let sources = equipment
            .values()
            .map(|id| {
                let item = self.items.get(id).ok_or(GameError::ItemUnavailable)?;
                let equipment = item
                    .spec
                    .equipment
                    .as_ref()
                    .ok_or(GameError::ItemUnavailable)?;
                Ok((*id, equipment.modifiers.clone()))
            })
            .collect::<Result<Vec<_>, GameError>>()?;
        let outcome = combat.set_equipment(sources).map_err(GameError::Creature)?;
        if outcome.died {
            self.next_item_id
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?;
        }
        Ok(Some(EquippedCreature { combat, outcome }))
    }

    fn set_actor_equipment(
        &mut self,
        actor: ActorId,
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<(), GameError> {
        self.settle_actor_time(actor)?;
        let Some(EquippedCreature { combat, outcome }) =
            self.equipment_candidate(actor, &equipment)?
        else {
            self.actors
                .get_mut(&actor)
                .ok_or(GameError::UnknownActor)?
                .equipment = equipment;
            return Ok(());
        };
        let death = if outcome.died {
            Some(self.prepare_death(actor, combat.clone())?)
        } else {
            None
        };
        let mut state = self.actors.get_mut(&actor).ok_or(GameError::UnknownActor)?;
        state.equipment = equipment;
        state.combat = Some(combat);
        let interrupted = state
            .pending
            .as_ref()
            .filter(|pending| {
                pending
                    .hold
                    .is_some_and(|id| outcome.canceled.contains(&id))
            })
            .cloned();
        if interrupted.is_some() {
            state.cancel_preparation();
            state.ready_at = self.tick;
        }
        drop(state);
        if let Some(death) = death {
            self.commit_death(death)?;
        } else if let Some(pending) = interrupted {
            self.event_log.push(CombatEvent::Interrupted {
                actor,
                intention: pending.intention,
            });
            if !self.is_ai(actor) {
                self.combat.input_boundaries.insert(actor);
            }
        }
        Ok(())
    }
    /// Recheck only live item references and socket eligibility. Captured effects,
    /// costs and timing remain owned by the original plan.
    pub(crate) fn item_work_available(&self, actor: ActorId, work: &Work) -> bool {
        let Some(a) = self.actors.get(&actor).filter(|actor| actor.alive()) else {
            return false;
        };
        let Some(item) = work.item() else {
            return false;
        };
        let Some(i) = self
            .items
            .get(&item)
            .filter(|item| item.location == ItemLocation::Carried(actor))
        else {
            return false;
        };
        match work {
            Work::Equip { slot, .. } => i.spec.equipment.as_ref().is_some_and(|equipment| {
                a.anatomy.slots.get(usize::from(slot.0)) == Some(&equipment.slot)
                    && !a.equipment.contains_key(slot)
                    && !a.equipment.values().any(|id| *id == item)
            }),
            Work::Unequip { .. } => {
                i.spec.equipment.is_some() && a.equipment.values().any(|id| *id == item)
            }
            Work::Drink { .. } => i.spec.consumable.is_some() && a.combat.is_some(),
            Work::UseAbility { .. } => false,
        }
    }
    fn item_work_duration(&self, actor: ActorId, work: Work) -> Result<u64, GameError> {
        let a = self
            .actors
            .get(&actor)
            .filter(|actor| actor.alive())
            .ok_or(GameError::UnknownActor)?;
        let item = work.item().ok_or(GameError::ItemUnavailable)?;
        let i = self
            .items
            .get(&item)
            .filter(|i| i.location == ItemLocation::Carried(actor))
            .ok_or(GameError::ItemUnavailable)?;
        let timing = &self.content.rules.constants().item_timing;
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
                {
                    return Err(GameError::ItemUnavailable);
                }
                let mut candidate = a.equipment.clone();
                candidate.insert(slot, item);
                self.equipment_candidate(actor, &candidate)?;
                timing.equip.turns(equipment.slot)
            }
            Work::Unequip { .. } => {
                if !a.equipment.values().any(|id| *id == item) {
                    return Err(GameError::ItemUnavailable);
                }
                let mut candidate = a.equipment.clone();
                candidate.retain(|_, equipped| *equipped != item);
                self.equipment_candidate(actor, &candidate)?;
                timing.unequip.turns(
                    i.spec
                        .equipment
                        .as_ref()
                        .ok_or(GameError::ItemUnavailable)?
                        .slot,
                )
            }
            Work::Drink { .. } => {
                if i.spec.consumable.is_none() || a.combat.is_none() {
                    return Err(GameError::ItemUnavailable);
                }
                timing.consume_turns
            }
            Work::UseAbility { .. } => return Err(GameError::ItemUnavailable),
        };
        a.turn_ticks
            .get()
            .checked_mul(turns)
            .filter(|duration| *duration <= crate::limits::NUMERIC as u64)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))
    }
    pub(crate) fn plan_work(
        &self,
        actor: ActorId,
        work: Work,
    ) -> Result<crate::abilities::Plan, GameError> {
        if let Some(preparation) = self.preparation(actor).filter(|p| p.plan.work == work) {
            if self.plan_still_available(&preparation.plan) {
                return Ok(preparation.plan.clone());
            }
            return Err(GameError::Rule(crate::abilities::RuleError::OutOfReach));
        }
        if let Work::UseAbility { id, target } = &work {
            return self.ability_plan(actor, id, *target);
        }
        let preparation = self.item_work_duration(actor, work.clone())?;
        let effects = if let Work::Drink { item } = &work {
            &self
                .items
                .get(item)
                .and_then(|item| item.spec.consumable.as_ref())
                .ok_or(GameError::ItemUnavailable)?
                .effects
        } else {
            &vec![]
        };
        let mut resolution = if effects.is_empty() {
            crate::abilities::ResolutionPlan {
                actor,
                target: actor,
                ability: None,
                preparation,
                recovery: 0,
                cost: None,
                reach: None,
                check: None,
                on_success: vec![],
                on_failure: vec![],
                descriptors: vec![],
                edge: 0,
            }
        } else {
            self.effect_plan(actor, effects)?
        };
        resolution.preparation = preparation;
        let plan = crate::abilities::Plan { work, resolution };
        if !plan.valid(&self.content.rules) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        Ok(plan)
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
        plan: crate::abilities::Plan,
        intention: Option<crate::IntentionId>,
    ) -> Result<(), GameError> {
        let actor = plan.actor;
        self.settle_actor_time(actor)?;
        let threats = if plan.work.item().is_some() {
            self.visible_hostiles(actor)
        } else {
            BTreeSet::new()
        };
        let mut candidate = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .clone();
        if candidate
            .pending
            .as_ref()
            .is_some_and(|p| p.plan.work != plan.work)
        {
            candidate.cancel_preparation();
        }
        let (remaining, hold, origin) = candidate
            .pending
            .as_ref()
            .filter(|p| p.plan.work == plan.work)
            .map_or((plan.preparation, intention, intention), |p| {
                (p.remaining, p.hold, p.origin)
            });
        if let Some(cost) = plan.cost {
            let owner = hold.ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
            candidate
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?
                .start_preparation_cost(owner, cost)?;
        }
        candidate.pending = Some(Preparation {
            intention,
            origin,
            hold: if plan.cost.is_some() { hold } else { None },
            plan,
            threats,
            remaining,
            started: self.tick,
            active: true,
        });
        self.actors.insert(actor, candidate);
        Ok(())
    }
    pub fn apply_effects(
        &mut self,
        actor: ActorId,
        effects: &[crate::rules::Effect],
    ) -> Result<bool, GameError> {
        let mut candidate = self.clone();
        let observed = candidate.apply_effects_in_transaction(actor, effects)?;
        *self = candidate;
        Ok(observed)
    }
    fn apply_effects_in_transaction(
        &mut self,
        actor: ActorId,
        effects: &[crate::rules::Effect],
    ) -> Result<bool, GameError> {
        if effects.is_empty() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        self.content
            .rules
            .validate_effects(effects)
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?;
        self.settle_actor_time(actor)?;
        let plan = self.effect_plan(actor, effects)?;
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let snapshot = crate::abilities::Snapshot::capture(&creature);
        let mut rng = self.rules_rng;
        let outcome = crate::abilities::resolve(&plan, &snapshot, &mut rng, &mut ());
        let observed = outcome.result == crate::abilities::OutcomeStatus::Applied;
        self.apply_outcome(outcome)?;
        self.rules_rng = rng;
        Ok(observed)
    }
    pub(crate) fn finish_item_work(
        &mut self,
        actor: ActorId,
        work: Work,
        intention: Option<crate::IntentionId>,
        observed: bool,
    ) -> Result<(), GameError> {
        let item = work.item().ok_or(GameError::ItemUnavailable)?;
        let event_index = self.event_log.len();
        match work {
            Work::Equip { slot, .. } => {
                let mut equipment = self
                    .actors
                    .get(&actor)
                    .ok_or(GameError::UnknownActor)?
                    .equipment
                    .clone();
                equipment.insert(slot, item);
                self.set_actor_equipment(actor, equipment)?;
            }
            Work::Unequip { .. } => {
                let mut equipment = self
                    .actors
                    .get(&actor)
                    .ok_or(GameError::UnknownActor)?
                    .equipment
                    .clone();
                equipment.retain(|_, equipped| *equipped != item);
                self.set_actor_equipment(actor, equipment)?;
            }
            Work::Drink { .. } => {
                let consumed = self.items.get(&item).ok_or(GameError::ItemUnavailable)?;
                let spec = consumed.spec.clone();
                if consumed.quantity == 1 {
                    self.items.remove(&item);
                } else {
                    self.items
                        .edit(item, |i| i.quantity -= 1)
                        .ok_or(GameError::ItemUnavailable)?;
                }
                if observed {
                    self.actors
                        .get_mut(&actor)
                        .ok_or(GameError::UnknownActor)?
                        .knowledge
                        .insert(spec.identity.clone());
                }
            }
            Work::UseAbility { .. } => return Err(GameError::ItemUnavailable),
        }
        self.event_log.insert(
            event_index,
            CombatEvent::ItemCompleted {
                actor,
                work,
                intention,
            },
        );
        Ok(())
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
            && a.pending.as_ref().is_none_or(|p| {
                a.alive()
                    && p.plan.valid(&self.content.rules)
                    && p.plan.actor == actor
                    && match p.plan.work {
                        Work::Drink { item } => self
                            .items
                            .get(&item)
                            .and_then(|item| item.spec.consumable.as_ref())
                            .is_some_and(|consumable| {
                                crate::abilities::item_effects_match_definition(
                                    &consumable.effects,
                                    &p.plan.on_success,
                                )
                            }),
                        _ => true,
                    }
                    && (p.plan.work.item().is_none()
                        || self.item_work_available(actor, &p.plan.work))
                    && p.started <= self.tick
                    && p.remaining <= p.plan.preparation
                    && p.started
                        .checked_add(p.remaining)
                        .and_then(|tick| tick.checked_add(p.plan.recovery))
                        .is_some()
                    && (!p.active || p.started + p.remaining >= self.actor_clock(actor))
                    && p.threats
                        .iter()
                        .all(|id| self.actors.contains_key(id) || self.detached_actor(*id))
                    && (p.plan.cost.is_none() || p.hold.is_some())
            })
    }
}

struct EquippedCreature {
    combat: crate::combat::CombatState,
    outcome: crate::creatures::RebuildOutcome,
}
