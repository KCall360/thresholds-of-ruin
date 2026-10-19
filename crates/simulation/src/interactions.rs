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

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct AnatomySpec {
    pub slots: Vec<EquipmentSlot>,
    pub capacities: crate::equipment_capacity::EquipmentCapacities,
    pub primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket,
}
impl<'de> Deserialize<'de> for AnatomySpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            slots: crate::bounded::Bounded<EquipmentSlot, { crate::limits::EQUIPMENT_SOCKETS }>,
            capacities: crate::equipment_capacity::EquipmentCapacities,
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            slots: fields.slots.0,
            capacities: fields.capacities,
            primary_attack_socket: fields.primary_attack_socket,
        };
        if result
            .primary_attack_socket
            .socket()
            .is_some_and(|slot| usize::from(slot.0) >= result.slots.len())
        {
            return Err(serde::de::Error::custom(
                "primary attack socket outside anatomy",
            ));
        }
        Ok(result)
    }
}
impl AnatomySpec {
    pub fn from_rules(rules: &crate::rules::Ruleset) -> Self {
        rules.constants().default_anatomy.clone()
    }
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        self.valid_with(|id| rules.equipment_slot(id))
    }
    pub(crate) fn valid_with<'a>(
        &self,
        slot_definition: impl Fn(&EquipmentSlot) -> Option<&'a crate::rules::EquipmentSlotDef>,
    ) -> bool {
        self.slots.len() <= crate::limits::EQUIPMENT_SOCKETS
            && self.slots.iter().all(|id| {
                slot_definition(id).is_some_and(|slot| {
                    slot.capacity.valid()
                        && slot
                            .capacity
                            .resource()
                            .is_none_or(|resource| self.capacities.get(resource).is_some())
                })
            })
            && self.primary_attack_socket.socket().is_none_or(|index| {
                self.slots
                    .get(usize::from(index.0))
                    .and_then(&slot_definition)
                    .is_some_and(|slot| slot.requires_attack)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSpec {
    pub slot: EquipmentSlot,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub armor: Option<crate::ArmorSpec>,
    pub attack: Option<crate::attacks::MeleeAttack>,
    pub weapon_profile: Option<crate::weapon_profiles::WeaponProfileSpec>,
    pub modifiers: Vec<crate::rules::Modifier>,
}
impl<'de> Deserialize<'de> for EquipmentSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            slot: EquipmentSlot,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            armor: Option<crate::ArmorSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            attack: Option<crate::attacks::MeleeAttack>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            weapon_profile: Option<crate::weapon_profiles::WeaponProfileSpec>,
            modifiers: Vec<crate::rules::Modifier>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let equipment = Self {
            slot: fields.slot,
            armor: fields.armor,
            attack: fields.attack,
            weapon_profile: fields.weapon_profile,
            modifiers: fields.modifiers,
        };
        if !equipment.valid_shape() {
            return Err(serde::de::Error::custom("invalid equipment"));
        }
        Ok(equipment)
    }
}
impl EquipmentSpec {
    pub fn valid_shape(&self) -> bool {
        self.armor.is_none_or(crate::ArmorSpec::valid)
            && (self.weapon_profile.is_none() || self.attack.is_some())
            && self
                .modifiers_with_armor(0)
                .is_some_and(|modifiers| crate::rules::validate_modifier_shapes(&modifiers).is_ok())
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
        if self
            .armor
            .is_some_and(|armor| !armor.valid() || class != ItemClass::Armor)
            || !slot.compatible_classes.contains(&class)
            || slot.requires_attack != self.attack.is_some()
        {
            return Err(crate::rules::RulesetError::Bound {
                field: "equipment_capability".into(),
            });
        }
        rules.validate_modifiers(&self.modifiers_with_armor(0).ok_or_else(|| {
            crate::rules::RulesetError::Bound {
                field: "armor_reduction".into(),
            }
        })?)?;
        if let Some(profile) = &self.weapon_profile {
            let validated =
                profile
                    .validate(rules)
                    .map_err(|_| crate::rules::RulesetError::Bound {
                        field: "weapon_profile".into(),
                    })?;
            if validated.maximum_grip_demand() > 1
                && matches!(
                    slot.capacity,
                    crate::equipment_capacity::EquipmentCapacityUse::None
                )
            {
                return Err(crate::rules::RulesetError::Bound {
                    field: "weapon_profile_capacity".into(),
                });
            }
        }
        if let Some(attack) = &self.attack {
            if attack.damage_enchantment() != 0 {
                return Err(crate::rules::RulesetError::Bound {
                    field: "equipment_capability".into(),
                });
            }
            rules.validate_damage(attack.damage())?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableSpec {
    pub discovery: crate::ItemDiscovery,
    pub effects: Vec<crate::rules::Effect>,
}
impl<'de> Deserialize<'de> for ConsumableSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            discovery: crate::ItemDiscovery,
            #[serde(deserialize_with = "crate::magic_items::read_readable_effects")]
            effects: Vec<crate::rules::Effect>,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let spec = Self {
            discovery: f.discovery,
            effects: f.effects,
        };
        if !spec.valid() {
            return Err(serde::de::Error::custom("invalid consumable capability"));
        }
        Ok(spec)
    }
}

/// Edibility is an explicit capability, independent of item class or identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EdibleSpec {
    pub nutrition: crate::rules::AmountExpr,
}
impl<'de> Deserialize<'de> for EdibleSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "read_edible_amount")]
            nutrition: crate::rules::AmountExpr,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            nutrition: fields.nutrition,
        })
    }
}
impl EdibleSpec {
    pub fn valid(&self) -> bool {
        match &self.nutrition {
            crate::rules::AmountExpr::Fixed { amount } => {
                (1..=crate::limits::NUMERIC as u32).contains(amount)
            }
            crate::rules::AmountExpr::Rolled { dice } => {
                crate::dice::DicePool::new(dice.count, dice.sides, dice.bonus)
                    .is_ok_and(|pool| pool.minimum() > 0)
            }
        }
    }
    pub fn effects(&self) -> [crate::rules::Effect; 1] {
        [crate::rules::Effect::Nourish {
            amount: self.nutrition.clone(),
        }]
    }
}

// Edible operands own exact named shapes at both nested boundaries without
// changing the decoding contract of unrelated authored effect expressions.
fn read_edible_amount<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<crate::rules::AmountExpr, D::Error> {
    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Fields {
        Fixed {
            amount: u32,
        },
        Rolled {
            #[serde(deserialize_with = "crate::bounded::named_map")]
            dice: crate::rules::DiceExpr,
        },
    }
    Ok(match crate::bounded::named_map(deserializer)? {
        Fields::Fixed { amount } => crate::rules::AmountExpr::Fixed { amount },
        Fields::Rolled { dice } => crate::rules::AmountExpr::Rolled { dice },
    })
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
    pub identification: Option<crate::IdentificationChoiceView>,
    pub completed: Vec<Work>,
    pub curse_blocked: Vec<ItemId>,
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
/// Remembered instance enchantments, not a resolved attack or damage total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectileBonusView {
    pub hit: i32,
    pub damage: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemInteractionView {
    pub item: ItemId,
    pub slot: Option<EquipmentSlotView>,
    pub equipped_slot: Option<EquipmentSlotId>,
    pub known_equipment: Option<EquipmentSpec>,
    pub known_projectile_bonus: Option<ProjectileBonusView>,
    pub known_cursed: Option<bool>,
    pub known_protected: Option<bool>,
    pub drinkable: bool,
    pub readable: bool,
    pub zappable: bool,
    pub charges: Option<u16>,
    pub edible: bool,
}
impl ConsumableSpec {
    pub fn valid(&self) -> bool {
        !self.effects.is_empty()
            && self.effects.len() <= crate::limits::CONSUMABLE_EFFECTS
            && self
                .effects
                .iter()
                .all(|effect| effect.valid_shape() && !effect.spatial())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Work {
    Escape {
        hold: crate::linked_hold::HoldId,
    },
    Respec {
        basis: crate::player_advancement::AdvancementBasis,
        replacement: crate::player_respec::ReplacementOperation,
        supplies: crate::supplies::SupplySelection,
    },
    UseAbility {
        id: crate::content_ids::AbilityId,
        invocation: crate::abilities::AbilityInvocation,
        supplies: crate::supplies::SupplySelection,
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
    Read {
        item: ItemId,
    },
    Zap {
        item: ItemId,
        direction: crate::abilities::StepDirection,
    },
    Eat {
        item: ItemId,
    },
}
impl<'de> serde::Deserialize<'de> for Work {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Escape {
                hold: crate::linked_hold::HoldId,
            },
            Respec {
                basis: crate::player_advancement::AdvancementBasis,
                replacement: crate::player_respec::ReplacementOperation,
                supplies: crate::supplies::SupplySelection,
            },
            UseAbility {
                id: crate::content_ids::AbilityId,
                invocation: crate::abilities::AbilityInvocation,
                supplies: crate::supplies::SupplySelection,
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
            Read {
                item: ItemId,
            },
            Zap {
                item: ItemId,
                direction: crate::abilities::StepDirection,
            },
            Eat {
                item: ItemId,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Escape { hold } => Self::Escape { hold },
            Fields::Respec {
                basis,
                replacement,
                supplies,
            } => Self::Respec {
                basis,
                replacement,
                supplies,
            },
            Fields::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id,
                invocation,
                supplies,
            },
            Fields::Equip { item, slot } => Self::Equip { item, slot },
            Fields::Unequip { item } => Self::Unequip { item },
            Fields::Drink { item } => Self::Drink { item },
            Fields::Read { item } => Self::Read { item },
            Fields::Zap { item, direction } => Self::Zap { item, direction },
            Fields::Eat { item } => Self::Eat { item },
        })
    }
}

impl Work {
    pub fn action(&self) -> Action {
        match self {
            Self::Escape { hold } => Action::Escape { hold: *hold },
            Self::Respec {
                basis,
                replacement,
                supplies,
            } => Action::Respec {
                basis: basis.clone(),
                replacement: replacement.clone(),
                supplies: supplies.clone(),
            },
            Self::UseAbility {
                id,
                invocation,
                supplies,
            } => Action::UseAbility {
                id: id.clone(),
                invocation: *invocation,
                supplies: supplies.clone(),
            },
            Self::Equip { item, slot } => Action::Equip {
                item: *item,
                slot: *slot,
            },
            Self::Unequip { item } => Action::Unequip { item: *item },
            Self::Drink { item } => Action::Drink { item: *item },
            Self::Read { item } => Action::Read { item: *item },
            Self::Zap { item, direction } => Action::Zap {
                item: *item,
                direction: *direction,
            },
            Self::Eat { item } => Action::Eat { item: *item },
        }
    }
    pub fn target(&self) -> Option<ActorId> {
        match self {
            Self::UseAbility { invocation, .. } => match invocation.aim {
                crate::abilities::AbilityAim::ActorTarget { target }
                | crate::abilities::AbilityAim::ContactTarget { target } => Some(target),
                _ => None,
            },
            Self::Escape { .. }
            | Self::Respec { .. }
            | Self::Equip { .. }
            | Self::Unequip { .. }
            | Self::Read { .. }
            | Self::Zap { .. }
            | Self::Drink { .. }
            | Self::Eat { .. } => None,
        }
    }
    pub fn item(&self) -> Option<ItemId> {
        match self {
            Self::Escape { .. } | Self::UseAbility { .. } | Self::Respec { .. } => None,
            Self::Equip { item, .. }
            | Self::Unequip { item }
            | Self::Read { item }
            | Self::Zap { item, .. }
            | Self::Drink { item }
            | Self::Eat { item } => Some(*item),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Escape { .. } => "escape",
            Self::UseAbility { .. } => "use ability",
            Self::Respec { .. } => "replace advancement",
            Self::Equip { .. } => "equip",
            Self::Unequip { .. } => "remove",
            Self::Drink { .. } => "drink",
            Self::Read { .. } => "read",
            Self::Zap { .. } => "zap",
            Self::Eat { .. } => "eat",
        }
    }
    pub(crate) fn structural_valid(&self, actor: ActorId) -> bool {
        match self {
            Self::Escape { hold } => hold.0 > 0 && hold.0 < u64::MAX,
            Self::Respec {
                basis, supplies, ..
            } => basis.valid() && basis.actor == actor && supplies.valid(1),
            Self::UseAbility {
                invocation,
                supplies,
                ..
            } => invocation.target(actor).0 > 0 && supplies.valid(crate::limits::SUPPLY_COSTS),
            Self::Equip { item, slot } => {
                item.0 > 0 && usize::from(slot.0) < crate::limits::EQUIPMENT_SOCKETS
            }
            Self::Unequip { item }
            | Self::Read { item }
            | Self::Zap { item, .. }
            | Self::Drink { item }
            | Self::Eat { item } => item.0 > 0,
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

    /// Structural occupancy is checked independently of the actor's optional
    /// combat component. This is the shared boundary for resource demand.
    fn equipment_capacity_state(
        &self,
        actor: ActorId,
        equipment: &BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Option<
        BTreeMap<
            crate::content_ids::EquipmentCapacityId,
            crate::equipment_capacity::EquipmentCapacityDeficit,
        >,
    > {
        let owner = self.actors.get(&actor)?;
        let anatomy = &owner.anatomy;
        let no_expertise = crate::expertise::ExpertiseOwnership::default();
        let expertises = owner.combat.as_ref().map_or(&no_expertise, |combat| {
            &combat.creature().derived().expertises
        });
        let mut demands = Vec::new();
        for (socket, item) in equipment {
            let slot = self
                .content
                .rules
                .equipment_slot(anatomy.slots.get(usize::from(socket.0))?)?;
            let item = self.items.get(item)?;
            if item.location != ItemLocation::Carried(actor)
                || item.spec.equipment.as_ref()?.slot != slot.id
            {
                return None;
            }
            if let crate::equipment_capacity::EquipmentCapacityUse::Shared { resource, units } =
                &slot.capacity
            {
                let profile = item
                    .spec
                    .equipment
                    .as_ref()?
                    .weapon_profile
                    .as_ref()
                    .map(|profile| profile.validate(&self.content.rules))
                    .transpose()
                    .ok()?;
                let minimum = profile
                    .as_ref()
                    .map_or(1, |profile| profile.minimum_grip_demand());
                let current = profile
                    .as_ref()
                    .map_or(1, |profile| profile.resolve(expertises).grip_demand());
                demands.push((
                    resource.clone(),
                    units.checked_mul(u16::from(minimum))?,
                    units.checked_mul(u16::from(current))?,
                ));
            }
        }
        crate::equipment_capacity::capacity_deficits(&anatomy.capacities, demands)
    }

    fn equipment_candidate(
        &self,
        actor: ActorId,
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<crate::lifecycle::ActorOutcome, GameError> {
        self.equipment_candidate_with_frozen_pool(actor, equipment, None)
    }
    fn equipment_candidate_with_frozen_pool(
        &self,
        actor: ActorId,
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
        frozen_pool: Option<&crate::content_ids::EquipmentCapacityId>,
    ) -> Result<crate::lifecycle::ActorOutcome, GameError> {
        let deficits = self
            .equipment_capacity_state(actor, &equipment)
            .ok_or(GameError::ItemUnavailable)?;
        let original = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        for (socket, item) in &equipment {
            if original.equipment.get(socket) == Some(item) {
                continue;
            }
            let slot = self
                .content
                .rules
                .equipment_slot(&original.anatomy.slots[usize::from(socket.0)])
                .ok_or(GameError::ItemUnavailable)?;
            if slot.capacity.resource().is_some_and(|resource| {
                Some(resource) != frozen_pool && deficits.contains_key(resource)
            }) {
                return Err(GameError::ItemUnavailable);
            }
        }
        let mut outcome = self.settled_actor_outcome(actor)?;
        self.refresh_equipment_outcome(&mut outcome, equipment)?;
        Ok(outcome)
    }

    /// Recompile equipment on the caller's owned state, preserving already-paid
    /// resources and lifecycle changes in the same completion transaction.
    pub(crate) fn refresh_equipment_outcome(
        &self,
        outcome: &mut crate::lifecycle::ActorOutcome,
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<(), GameError> {
        let actor = outcome.id;
        self.equipment_capacity_state(actor, &equipment)
            .ok_or(GameError::ItemUnavailable)?;
        if let Some(combat) = outcome.state.combat.as_mut() {
            let sources = equipment
                .iter()
                .map(|(socket, id)| {
                    let item = self.items.get(id).ok_or(GameError::ItemUnavailable)?;
                    item.equipment_source(*id, *socket, &self.content.rules)
                        .ok_or(GameError::ItemUnavailable)
                })
                .collect::<Result<Vec<_>, GameError>>()?;
            let context = self.equipment_recovery_context(actor, &sources)?;
            let change = combat
                .set_equipment_in_context(sources, context)
                .map_err(GameError::Creature)?;
            if change.died {
                self.next_item_id
                    .checked_add(1)
                    .ok_or(GameError::IdentityExhausted)?;
            }
        }
        outcome.state.equipment = equipment;
        Ok(())
    }

    fn set_actor_equipment(
        &mut self,
        actor: ActorId,
        equipment: BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Result<(), GameError> {
        let outcome = self.equipment_candidate(actor, equipment)?;
        let prepared = self.prepare_actor_outcome(outcome)?;
        self.publish_actor_effect(prepared, false)
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
            Work::Read { .. } => {
                i.spec.readable.is_some() && a.combat.is_some() && a.identification.is_none()
            }
            Work::Zap { .. } => {
                i.spec.device.is_some() && i.charges.is_some() && a.combat.is_some()
            }
            Work::Eat { .. } => i.spec.edible.is_some() && a.combat.is_some(),
            Work::Escape { .. } | Work::UseAbility { .. } | Work::Respec { .. } => false,
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
            Work::Read { .. } => {
                if i.spec.readable.is_none() || a.combat.is_none() || a.identification.is_some() {
                    return Err(GameError::ItemUnavailable);
                }
                timing.consume_turns
            }
            Work::Zap { .. } => {
                if i.spec.device.is_none() || i.charges.is_none() || a.combat.is_none() {
                    return Err(GameError::ItemUnavailable);
                }
                timing.consume_turns
            }
            Work::Eat { .. } => {
                if i.spec.edible.is_none() || a.combat.is_none() {
                    return Err(GameError::ItemUnavailable);
                }
                timing.consume_turns
            }
            Work::Escape { .. } | Work::UseAbility { .. } | Work::Respec { .. } => {
                return Err(GameError::ItemUnavailable);
            }
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
        if let Work::Escape { hold } = &work {
            return self.escape_plan(actor, *hold);
        }
        if let Work::UseAbility {
            id,
            invocation,
            supplies,
        } = &work
        {
            let plan = self.ability_plan_with_supplies(actor, id, *invocation, supplies.clone())?;
            self.item_allocation(&plan)?;
            return Ok(plan);
        }
        if let Work::Respec {
            basis,
            replacement,
            supplies,
        } = work
        {
            return self.respec_plan(actor, basis, replacement, supplies);
        }
        let preparation = self.item_work_duration(actor, work.clone())?;
        if let Work::Zap { item, direction } = work {
            let ray = self.compile_device_ray_plan(actor, item, direction, preparation)?;
            self.item_allocation(&ray)?;
            return Ok(ray);
        }
        let plan = match work {
            Work::Escape { .. } | Work::Respec { .. } | Work::Zap { .. } => {
                unreachable!("distinct frozen plan")
            }
            Work::Equip { item, slot } => {
                let allocation = self.capture_equip_allocation(actor, item, slot)?;
                crate::abilities::Plan::Equip {
                    actor,
                    item,
                    slot,
                    preparation: if allocation.quickdraw(&self.content.rules) {
                        0
                    } else {
                        preparation
                    },
                    allocation,
                }
            }
            Work::Unequip { item } => crate::abilities::Plan::Unequip {
                actor,
                item,
                preparation,
            },
            Work::Eat { item } => {
                let edible = self
                    .items
                    .get(&item)
                    .and_then(|item| item.spec.edible.as_ref())
                    .ok_or(GameError::ItemUnavailable)?;
                crate::abilities::Plan::Eat {
                    actor,
                    item,
                    preparation,
                    effects: self.effect_plan(actor, &edible.effects())?,
                }
            }
            Work::Read { item } => {
                let readable = self
                    .items
                    .get(&item)
                    .and_then(|i| i.spec.readable.as_ref())
                    .ok_or(GameError::ItemUnavailable)?;
                crate::abilities::Plan::Read {
                    actor,
                    item,
                    preparation,
                    effects: self.effect_plan(actor, &readable.effects)?,
                    identify: readable.identify,
                    discovery: readable.discovery,
                }
            }
            Work::Drink { item } => {
                let effects = &self
                    .items
                    .get(&item)
                    .and_then(|item| item.spec.consumable.as_ref())
                    .ok_or(GameError::ItemUnavailable)?
                    .effects;
                crate::abilities::Plan::Drink {
                    discovery: self.items[&item]
                        .spec
                        .consumable
                        .as_ref()
                        .expect("validated consumable")
                        .discovery,
                    actor,
                    item,
                    preparation,
                    effects: self.effect_plan(actor, effects)?,
                }
            }
            Work::UseAbility {
                id,
                invocation,
                supplies,
            } => return self.ability_plan_with_supplies(actor, &id, invocation, supplies),
        };
        self.item_allocation(&plan)?;
        if !plan.valid(&self.content.rules) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        Ok(plan)
    }
    pub(crate) fn visible_hostiles(&self, actor: ActorId) -> BTreeSet<ActorId> {
        let Ok(scene) = self.scene(actor) else {
            return BTreeSet::new();
        };
        self.hostiles_in_scene(actor, &scene)
    }
    pub(crate) fn hostiles_in_scene(
        &self,
        actor: ActorId,
        scene: &[tor_world::SightCell],
    ) -> BTreeSet<ActorId> {
        let Ok((_, sight)) = self.scene_snapshot(actor) else {
            return BTreeSet::new();
        };
        self.hostiles_in_projection(actor, scene, &sight, None)
    }
    pub(crate) fn hostiles_in_projection(
        &self,
        actor: ActorId,
        scene: &[tor_world::SightCell],
        sight: &[tor_world::SightCell],
        placement: Option<crate::abilities::Placement>,
    ) -> BTreeSet<ActorId> {
        self.perception_projection_at(actor, scene, sight, placement)
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, row)| {
                *id != actor
                    && self.alive(*id)
                    && self.perceived_hostile(actor, *id, row.disclosure)
            })
            .map(|(id, _)| id)
            .collect()
    }

    pub(crate) fn visible_threats(
        &self,
        actor: ActorId,
    ) -> BTreeSet<crate::perception::PerceivedThreat> {
        let Ok((scene, sight)) = self.scene_snapshot(actor) else {
            return BTreeSet::new();
        };
        self.perception_projection(actor, &scene, &sight)
            .unwrap_or_default()
            .into_iter()
            .filter(|(id, row)| {
                *id != actor
                    && self.alive(*id)
                    && self.perceived_hostile(actor, *id, row.disclosure)
            })
            .filter_map(|(actor, row)| {
                Some(crate::perception::PerceivedThreat {
                    actor,
                    reference: row.disclosure.reference()?,
                })
            })
            .collect()
    }
    pub(crate) fn start_work(
        &mut self,
        plan: crate::abilities::Plan,
        intention: Option<crate::IntentionId>,
    ) -> Result<(), GameError> {
        // Owned/frozen operands are admitted against pinned source semantics,
        // never today's equipment. Reject before time, retirement, or payment.
        if !plan.valid(self.content.rules()) || !self.plan_source_valid(&plan) {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let actor = plan.actor();
        if plan
            .as_ability()
            .is_some_and(|ability| !self.frozen_weapon_available(actor, &ability.weapon))
        {
            return Err(GameError::ItemUnavailable);
        }
        self.settle_actor_time(actor)?;
        if let crate::abilities::Plan::Escape(escape) = &plan {
            self.settle_actor_time(escape.holder)?;
            if !self.escape_plan_available(escape) {
                return Err(GameError::Rule(crate::abilities::RuleError::OutOfReach));
            }
        }
        if matches!(plan, crate::abilities::Plan::Respec(_)) && !self.respec_environment_safe(actor)
        {
            return Err(GameError::ActorBusy);
        }
        let resuming = self
            .preparation(actor)
            .is_some_and(|pending| pending.plan == plan);
        if self
            .preparation(actor)
            .is_some_and(|pending| pending.has_prefix() && pending.plan != plan)
        {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        if !resuming && self.preparation(actor).is_some() {
            let mut retirement = self.actor_outcome(actor)?;
            retirement.discard(self.tick)?;
            self.apply_outcome(retirement)?;
        }
        let item_payment = if resuming {
            self.preparation(actor)
                .expect("same preparation")
                .item_payment
                .clone()
        } else {
            self.reserve_item_payment(&plan, intention)?
        };
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
            self.visible_threats(actor)
        } else {
            BTreeSet::new()
        };
        if threats.len() > crate::limits::PUBLIC_AIM_OCCURRENCES {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
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
        let paid = matches!(progress, crate::combat::PreparationProgress::Ability { next_strike, .. } if next_strike > 0);
        if let Some(cost) = plan.cost().filter(|_| !paid) {
            let owner = hold.ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
            let started = candidate
                .combat
                .as_mut()
                .ok_or(GameError::UnknownActor)?
                .start_preparation_cost(owner, cost)?;
            if self.survival_diagnostics.is_some() && started == crate::costs::StartCost::Started {
                for cost in cost.entries() {
                    crate::survival_diagnostics::checked_add(
                        outcome.survival.paid_start.get_mut(cost.resource),
                        u64::from(cost.start),
                        &mut outcome.survival.incomplete,
                    );
                }
            }
        }
        if !resuming {
            if let Some(ability) = plan.as_ability() {
                if ability.ambush.is_some() {
                    let owner = origin
                        .ok_or(GameError::Schedule(crate::ScheduleError::InvalidIntention))?;
                    if !candidate.guise.ambush_available() {
                        return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
                    }
                    candidate.guise.ambush =
                        crate::perception::AmbushState::Reserved { origin: owner };
                }
            }
            // Item-defined rays own hostile intent without making their private
            // definition an actor ability or acquiring a cooldown entitlement.
            let interaction_source = plan
                .as_ray()
                .map(|ray| &ray.ability)
                .or_else(|| plan.ability_id());
            if interaction_source.is_some_and(|ability| {
                self.content
                    .rules
                    .ability(ability)
                    .is_some_and(|definition| {
                        definition.interaction == crate::rules::Interaction::HostileToTarget
                    })
            }) {
                candidate.guise.reveal();
            }
            if let Some(ability) = plan.ability_id() {
                if let Some(cooldown) = plan.ability_cooldown() {
                    candidate
                        .combat
                        .as_mut()
                        .ok_or(GameError::UnknownActor)?
                        .start_preparation_cooldown(ability, cooldown.get())?;
                }
            }
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
            item_payment,
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
        self.apply_outcome(outcome)?;
        self.ensure_preparation_residency(actor)
    }
    pub fn apply_effects(
        &mut self,
        actor: ActorId,
        effects: &[crate::rules::Effect],
    ) -> Result<bool, GameError> {
        self.with_effect_publication(|candidate| {
            candidate.apply_effects_in_transaction(actor, effects)
        })
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
        let resolution = crate::abilities::ResolutionPlan::Effects {
            actor,
            target: actor,
            effects: &plan,
        };
        let snapshot = crate::abilities::Snapshot::capture(&creature)
            .with_context(self.resolution_context(resolution, actor, actor)?);
        let mut rng = self.rules_rng;
        let outcome = crate::abilities::resolve(resolution, &snapshot, &mut rng, &mut ())?;
        let observed = outcome.result == crate::abilities::OutcomeStatus::Applied;
        self.apply_outcome(outcome)?;
        self.rules_rng = rng;
        Ok(observed)
    }
    pub(crate) fn finish_item_work(
        &mut self,
        actor: ActorId,
        plan: &crate::abilities::Plan,
        intention: Option<crate::IntentionId>,
        observed: bool,
        consumed_identity: Option<String>,
    ) -> Result<(), GameError> {
        use crate::lifecycle::{ItemChange, ItemOutcome};
        let work = plan.work();
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
                let crate::abilities::Plan::Equip { allocation, .. } = plan else {
                    return Err(GameError::ItemUnavailable);
                };
                if !self.frozen_equip_available(actor, item, slot, allocation) {
                    return Err(GameError::ItemUnavailable);
                }
                let mut outcome = self.equipment_candidate_with_frozen_pool(
                    actor,
                    equipment,
                    allocation.resource(self, actor, slot),
                )?;
                if let Some(gear) = self.items.get(&item).filter(|item| {
                    item.spec
                        .equipment
                        .as_ref()
                        .is_some_and(|gear| gear.armor.is_some())
                }) {
                    outcome
                        .state
                        .known_equipment
                        .learn_numbers(item, gear.condition)?;
                }
                ItemChange::Equipment(outcome)
            }
            Work::Unequip { .. } => {
                if self
                    .items
                    .get(&item)
                    .is_some_and(|item| item.condition.cursed)
                {
                    let mut outcome = self.actor_outcome(actor)?;
                    outcome.state.known_equipment.learn_curse(item, true)?;
                    return self.apply_outcome(ItemOutcome {
                        actor,
                        work,
                        intention,
                        change: ItemChange::CurseBlocked(outcome),
                    });
                }
                let mut equipment = self
                    .actors
                    .get(&actor)
                    .ok_or(GameError::UnknownActor)?
                    .equipment
                    .clone();
                equipment.retain(|_, equipped| *equipped != item);
                ItemChange::Equipment(self.equipment_candidate(actor, equipment)?)
            }
            Work::Drink { .. } | Work::Eat { .. } | Work::Read { .. } | Work::Zap { .. } => {
                let mut outcome = self.actor_outcome(actor)?;
                if observed {
                    outcome
                        .state
                        .knowledge
                        .insert(consumed_identity.ok_or(GameError::ItemUnavailable)?);
                }
                ItemChange::Consumed { actor: outcome }
            }
            Work::Escape { .. } | Work::UseAbility { .. } | Work::Respec { .. } => {
                return Err(GameError::ItemUnavailable);
            }
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
                && entry.qualification == token.qualification
                && entry.order == token.order
                && entry.generation == token.generation
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
            && state.guise.claim_valid(state.pending.as_ref())
            && state.combat.as_ref().is_none_or(|combat| {
                combat.creature().conditions().instances().all(|entry| {
                    entry.qualification.as_ref().is_none_or(|qualification| {
                        self.actors.contains_key(&qualification.opponent)
                            || self.detached_actor(qualification.opponent)
                    }) && entry.reserved_by.is_none_or(|owner| {
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
            && self.equipment_capacity_state(actor, &a.equipment).is_some()
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
                    // Use committed state, not projected recovery, for this historical
                    // proof. A dead or detached target may have skipped a lethal rider.
                    && p.plan.as_ability().is_none_or(|plan| {
                        !self.actors.get(&plan.target)
                            .and_then(|target| target.combat.as_ref())
                            .is_some_and(|target| !target.creature().health().dead())
                            || p.progress.conditional_receipts_complete(plan)
                    })
                    && p.plan.actor() == actor
                    && match p.plan.work() {
                        Work::Read {item}=>self.items.get(&item).and_then(|i|i.spec.readable.as_ref()).is_some_and(|s|matches!(&p.plan,crate::abilities::Plan::Read {effects,identify,discovery,..} if *identify==s.identify&&*discovery==s.discovery&&crate::abilities::item_effects_match_definition(&s.effects,effects))),
                        Work::Zap {item,..}=>self.items.get(&item).is_some_and(|i|matches!(&p.plan,crate::abilities::Plan::Zap {charges,ray,..} if i.charges==Some(*charges)&&i.spec.device.as_ref().is_some_and(|d|d.ability==ray.ability))),
                        Work::Eat { item } => self
                            .items
                            .get(&item)
                            .and_then(|item| item.spec.edible.as_ref())
                            .is_some_and(|edible| {
                                crate::abilities::item_effects_match_definition(
                                    &edible.effects(),
                                    p.plan.on_success(),
                                )
                            }),
                        Work::Drink { item } => self
                            .items
                            .get(&item)
                            .and_then(|item| item.spec.consumable.as_ref())
                            .is_some_and(|consumable| {
                                crate::abilities::item_effects_match_definition(
                                    &consumable.effects,
                                    p.plan.on_success(),
                                ) && matches!(&p.plan,crate::abilities::Plan::Drink{discovery,..} if *discovery==consumable.discovery)
                            }),
                        _ => true,
                    }
                    && (p.plan.work().item().is_none()
                        || self.item_work_available(actor, &p.plan.work()))
                    && self.preparation_items_valid(p, true)
                    && p.schedule_valid(self.actor_clock(actor), a.ready_at)
                    && p.threats.len() <= crate::limits::PUBLIC_AIM_OCCURRENCES
                    && p.threats
                        .iter()
                        .all(|threat| threat.actor != actor && (self.actors.contains_key(&threat.actor) || self.detached_actor(threat.actor)))
            })
    }
}
