//! Anatomy, ordinary equipment actions and reusable immediate effect definitions.
use crate::combat::Preparation;
use crate::{Action, ActorId, Game, GameError, ItemClass, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub use crate::content_ids::EquipmentSlot;

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
    pub fn from_rules(rules: &crate::rules::Ruleset) -> Self {
        Self {
            slots: rules.constants().default_anatomy.clone(),
        }
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        self.slots.len() <= crate::limits::EQUIPMENT_SOCKETS
            && self
                .slots
                .iter()
                .all(|id| rules.equipment_slot(id).is_some())
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
    pub fn valid_shape(&self) -> bool {
        crate::rules::validate_modifier_shapes(&self.modifiers).is_ok()
    }
    pub fn validate_rules(
        &self,
        class: ItemClass,
        rules: &crate::rules::Ruleset,
    ) -> Result<(), crate::rules::RulesetError> {
        let slot = rules.equipment_slot(&self.slot).ok_or_else(|| {
            crate::rules::RulesetError::UnknownReference {
                category: "equipment_slot",
                id: self.slot.to_string(),
            }
        })?;
        if !slot.compatible_classes.contains(&class)
            || slot.requires_attack != self.attack.is_some()
        {
            return Err(crate::rules::RulesetError::Bound {
                field: "equipment_capability".into(),
            });
        }
        rules.validate_modifiers(&self.modifiers)?;
        if let Some(attack) = &self.attack {
            rules.validate_damage(attack.damage())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub effects: Vec<crate::rules::Effect>,
}

/// Data disclosed for a validated equipment-slot content reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentSlotView {
    pub id: EquipmentSlot,
    pub name: String,
    pub equipped_phrase: String,
    pub requires_attack: bool,
}
impl From<&crate::rules::EquipmentSlotDef> for EquipmentSlotView {
    fn from(value: &crate::rules::EquipmentSlotDef) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            equipped_phrase: value.equipped_phrase.clone(),
            requires_attack: value.requires_attack,
        }
    }
}

/// Controlled actor's anatomy and carried-item affordances. Equipment statistics
/// remain absent until that actor knows the item's identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InteractionView {
    pub completed: Vec<Work>,
    pub slots: Vec<EquipmentSlotView>,
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
    pub slot: Option<EquipmentSlotView>,
    pub equipped_slot: Option<EquipmentSlotId>,
    pub known_equipment: Option<EquipmentSpec>,
    pub drinkable: bool,
}
impl ConsumableSpec {
    pub fn valid(&self) -> bool {
        !self.effects.is_empty()
            && self.effects.len() <= crate::limits::CONSUMABLE_EFFECTS
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
            Self::Equip { item, slot } => {
                item.0 > 0 && usize::from(slot.0) < crate::limits::EQUIPMENT_SOCKETS
            }
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
        if !anatomy.valid(&self.content.rules)
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
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<crate::lifecycle::ActorOutcome, GameError> {
        let mut outcome = self.settled_actor_outcome(actor)?;
        if let Some(combat) = outcome.state.combat.as_mut() {
            let sources = equipment
                .iter()
                .map(|(socket, id)| {
                    let item = self.items.get(id).ok_or(GameError::ItemUnavailable)?;
                    let equipment = item
                        .spec
                        .equipment
                        .as_ref()
                        .ok_or(GameError::ItemUnavailable)?;
                    Ok(crate::creatures::EquipmentSource {
                        item: *id,
                        socket: *socket,
                        modifiers: equipment.modifiers.clone(),
                        attack: equipment.attack.clone(),
                    })
                })
                .collect::<Result<Vec<_>, GameError>>()?;
            let change = combat.set_equipment(sources).map_err(GameError::Creature)?;
            if change.died {
                self.next_item_id
                    .checked_add(1)
                    .ok_or(GameError::IdentityExhausted)?;
            }
        }
        outcome.state.equipment = equipment;
        Ok(outcome)
    }

    fn set_actor_equipment(
        &mut self,
        actor: ActorId,
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<(), GameError> {
        let outcome = self.equipment_candidate(actor, equipment)?;
        self.apply_outcome(outcome)
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
                self.equipment_candidate(actor, candidate)?;
                self.content
                    .rules
                    .equipment_slot(&equipment.slot)
                    .ok_or(GameError::ItemUnavailable)?
                    .equip_turns
            }
            Work::Unequip { .. } => {
                if !a.equipment.values().any(|id| *id == item) {
                    return Err(GameError::ItemUnavailable);
                }
                let mut candidate = a.equipment.clone();
                candidate.retain(|_, equipped| *equipped != item);
                self.equipment_candidate(actor, candidate)?;
                self.content
                    .rules
                    .equipment_slot(
                        &i.spec
                            .equipment
                            .as_ref()
                            .ok_or(GameError::ItemUnavailable)?
                            .slot,
                    )
                    .ok_or(GameError::ItemUnavailable)?
                    .unequip_turns
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
        if let Some(preparation) = self.preparation(actor).filter(|p| p.plan.work() == work) {
            if self.preparation_still_available(preparation, None) {
                return Ok(preparation.plan.clone());
            }
            return Err(GameError::Rule(crate::abilities::RuleError::OutOfReach));
        }
        if let Work::UseAbility { id, target } = &work {
            return self.ability_plan(actor, id, *target);
        }
        let preparation = self.item_work_duration(actor, work.clone())?;
        let plan = match work {
            Work::Equip { item, slot } => crate::abilities::Plan::Equip {
                actor,
                item,
                slot,
                preparation,
            },
            Work::Unequip { item } => crate::abilities::Plan::Unequip {
                actor,
                item,
                preparation,
            },
            Work::Drink { item } => {
                let effects = &self
                    .items
                    .get(&item)
                    .and_then(|item| item.spec.consumable.as_ref())
                    .ok_or(GameError::ItemUnavailable)?
                    .effects;
                crate::abilities::Plan::Drink {
                    actor,
                    item,
                    preparation,
                    effects: self.effect_plan(actor, effects)?,
                }
            }
            Work::UseAbility { id, target } => return self.ability_plan(actor, &id, target),
        };
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
        let actor = plan.actor();
        self.settle_actor_time(actor)?;
        let resuming = self
            .preparation(actor)
            .is_some_and(|pending| pending.plan == plan);
        if self
            .preparation(actor)
            .is_some_and(|pending| pending.has_prefix() && pending.plan != plan)
        {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        // Validation/admission only previews token identities. The same owned
        // plan is claimed here, inside the action transaction, before work starts.
        for holder_id in plan
            .tokens()
            .filter(|_| !resuming)
            .map(|token| token.holder)
            .collect::<BTreeSet<_>>()
        {
            let tokens: Vec<_> = plan
                .tokens()
                .filter(|token| token.holder == holder_id)
                .cloned()
                .collect();
            let mut holder = self.settled_actor_outcome(holder_id)?;
            holder
                .state
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?
                .reserve_tokens(&tokens, actor)?;
            self.apply_outcome(holder)?;
        }
        let threats = if plan.work().item().is_some() {
            self.visible_hostiles(actor)
        } else {
            BTreeSet::new()
        };
        let mut outcome = self.actor_outcome(actor)?;
        let candidate = &mut outcome.state;
        if candidate
            .pending
            .as_ref()
            .is_some_and(|p| p.plan.work() != plan.work())
        {
            candidate.cancel_preparation();
        }
        let (remaining, hold, origin, progress) = candidate
            .pending
            .as_ref()
            .filter(|p| p.plan.work() == plan.work())
            .map_or(
                (
                    plan.preparation(),
                    intention,
                    intention,
                    crate::combat::PreparationProgress::new(&plan),
                ),
                |p| (p.remaining_at(self.tick), p.hold, p.origin, p.progress),
            );
        let paid = matches!(progress, crate::combat::PreparationProgress::Ability { next_strike } if next_strike > 0);
        if let Some(cost) = plan.cost().filter(|_| !paid) {
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
            hold: if plan.cost().is_some() && !paid {
                hold
            } else {
                None
            },
            progress,
            plan,
            threats,
            remaining,
            started: self.tick,
            active: true,
        });
        let pending = candidate.pending.as_ref().expect("installed preparation");
        self.tick
            .checked_add(pending.completion_delay(self.tick)?)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        candidate.ready_at = self
            .tick
            .checked_add(remaining)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        self.apply_outcome(outcome)
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
        let outcome = crate::abilities::resolve(
            crate::abilities::ResolutionPlan::Effects {
                actor,
                target: actor,
                effects: &plan,
            },
            &snapshot,
            &mut rng,
            &mut (),
        );
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
        use crate::lifecycle::{ItemChange, ItemOutcome};
        let item = work.item().ok_or(GameError::ItemUnavailable)?;
        let change = match work {
            Work::Equip { slot, .. } => {
                let mut equipment = self
                    .actors
                    .get(&actor)
                    .ok_or(GameError::UnknownActor)?
                    .equipment
                    .clone();
                equipment.insert(slot, item);
                ItemChange::Equipment(self.equipment_candidate(actor, equipment)?)
            }
            Work::Unequip { .. } => {
                let mut equipment = self
                    .actors
                    .get(&actor)
                    .ok_or(GameError::UnknownActor)?
                    .equipment
                    .clone();
                equipment.retain(|_, equipped| *equipped != item);
                ItemChange::Equipment(self.equipment_candidate(actor, equipment)?)
            }
            Work::Drink { .. } => {
                let consumed = self.items.get(&item).ok_or(GameError::ItemUnavailable)?;
                let mut outcome = self.actor_outcome(actor)?;
                if observed {
                    outcome
                        .state
                        .knowledge
                        .insert(consumed.spec.identity.clone());
                }
                ItemChange::Consume {
                    actor: outcome,
                    item,
                }
            }
            Work::UseAbility { .. } => return Err(GameError::ItemUnavailable),
        };
        self.apply_outcome(ItemOutcome {
            actor,
            work,
            intention,
            change,
        })
    }
    pub(crate) fn token_reservations_valid(&self, actor: ActorId, allow_detached: bool) -> bool {
        let state = &self.actors[&actor];
        let named = |entry: &crate::conditions::ConditionInstance,
                     token: &crate::conditions::TokenReservation| {
            entry.condition == token.condition
                && entry.source == token.source
                && entry.order == token.order
        };
        let claims_valid = state.pending.as_ref().is_none_or(|preparation| {
            preparation.plan.tokens().all(|token| {
                self.actors.get(&token.holder).is_some_and(|holder| {
                    !holder.alive()
                        || holder.combat.as_ref().is_some_and(|combat| {
                            combat.creature().conditions().instances().any(|entry| {
                                named(entry, token) && entry.reserved_by == Some(actor)
                            })
                        })
                }) || (allow_detached && self.detached_actor(token.holder))
            })
        });
        claims_valid
            && state.combat.as_ref().is_none_or(|combat| {
                combat.creature().conditions().instances().all(|entry| {
                    entry.reserved_by.is_none_or(|owner| {
                        self.actors.get(&owner).is_some_and(|preparer| {
                            preparer.pending.as_ref().is_some_and(|preparation| {
                                preparation
                                    .plan
                                    .tokens()
                                    .any(|token| token.holder == actor && named(entry, token))
                            })
                        }) || (allow_detached && self.detached_actor(owner))
                    })
                })
            })
    }

    pub(crate) fn work_state_valid(&self, actor: ActorId) -> bool {
        let a = &self.actors[&actor];
        let equipped: BTreeSet<_> = a.equipment.values().collect();
        a.anatomy.valid(&self.content.rules)
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
                    && self.plan_source_valid(&p.plan)
                    && p.plan.actor() == actor
                    && match p.plan.work() {
                        Work::Drink { item } => self
                            .items
                            .get(&item)
                            .and_then(|item| item.spec.consumable.as_ref())
                            .is_some_and(|consumable| {
                                crate::abilities::item_effects_match_definition(
                                    &consumable.effects,
                                    p.plan.on_success(),
                                )
                            }),
                        _ => true,
                    }
                    && (p.plan.work().item().is_none()
                        || self.item_work_available(actor, &p.plan.work()))
                    && p.schedule_valid(self.actor_clock(actor), a.ready_at)
                    && p.threats
                        .iter()
                        .all(|id| self.actors.contains_key(id) || self.detached_actor(*id))
            })
    }
}
