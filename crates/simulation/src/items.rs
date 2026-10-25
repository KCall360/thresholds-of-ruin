use crate::{ActorId, Game, GameError, Item, ItemId, ItemLocation, OutcomeKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_world::Location;

#[cfg(test)]
#[path = "item_stack_tests.rs"]
mod stack_tests;

#[cfg(test)]
#[path = "scare_item_tests.rs"]
mod scare_tests;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurrencyKind {
    Gold,
}

/// An uninterrupted candidate owns this plan. Exact witnesses prevent a stale
/// merge identity, changed reservation, or allocator reuse from moving items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedTransfer {
    source: ItemId,
    expected_source: Item,
    result: ItemId,
    merge: Option<ItemId>,
    expected_destination: Option<Item>,
    inherited: Option<(ActorId, crate::equipment_knowledge::InheritedEquipmentFact)>,
    destination: ItemLocation,
    quantity: u64,
    expected_next_item_id: u64,
    condition_after: crate::armor::ItemCondition,
    crumble: bool,
}

impl PreparedTransfer {
    pub(crate) fn result(&self) -> ItemId {
        self.result
    }
    pub(crate) fn outcome(&self, taking: bool) -> OutcomeKind {
        if self.crumble {
            OutcomeKind::Crumbled {
                item: self.source,
                quantity: self.quantity,
            }
        } else if taking {
            OutcomeKind::Taken {
                item: self.source,
                result: self.result,
                quantity: self.quantity,
            }
        } else {
            OutcomeKind::Dropped {
                item: self.source,
                result: self.result,
                quantity: self.quantity,
            }
        }
    }
}

/// Value identity for homogeneous stacks. Location, motion and reservations are
/// transfer constraints rather than interchangeable per-unit state.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ItemStackKey {
    spec: tor_world::Shared<ItemSpec>,
    condition: crate::armor::ItemCondition,
    charges: Option<u16>,
}

impl Item {
    pub(crate) fn stack_key(&self) -> Option<ItemStackKey> {
        // Corpse appearance is exact provenance, never an interchangeable unit.
        (self.spec.stackable && self.corpse.is_none()).then(|| ItemStackKey {
            spec: self.spec.clone(),
            condition: self.condition,
            charges: self.charges,
        })
    }

    pub(crate) fn stacks_with(&self, other: &Self) -> bool {
        self.stack_key()
            .is_some_and(|key| other.stack_key() == Some(key))
    }
}

/// Authoritative identity and physical stack properties. Never a client DTO.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub magical: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub held_light: Option<crate::HeldLightSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub currency: Option<CurrencyKind>,
    pub equipment: Option<crate::EquipmentSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub projectile: Option<crate::projectile_profiles::ProjectileSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub equipment_generation: Option<crate::EquipmentGeneration>,
    pub consumable: Option<crate::ConsumableSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub readable: Option<crate::ReadableSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub ground_scare: Option<crate::control_scrolls::ScareSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub device: Option<crate::DeviceSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub edible: Option<crate::EdibleSpec>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub supply: Option<crate::content_ids::SupplyId>,
    pub class: ItemClass,
    pub archetype: String,
    pub identity: String,
    pub name: String,
    pub appearance: String,
    pub concealed: bool,
    pub stackable: bool,
    pub properties: BTreeMap<String, String>,
    /// The asset clients draw it with. For a concealed item it names only
    /// what every identity sharing its appearances looks like, so it never
    /// discloses the identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
}
impl<'de> serde::Deserialize<'de> for ItemSpec {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            magical: bool,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            held_light: Option<crate::HeldLightSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            currency: Option<CurrencyKind>,
            equipment: Option<crate::EquipmentSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            projectile: Option<crate::projectile_profiles::ProjectileSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            equipment_generation: Option<crate::EquipmentGeneration>,
            consumable: Option<crate::ConsumableSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            readable: Option<crate::ReadableSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            ground_scare: Option<crate::control_scrolls::ScareSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            device: Option<crate::DeviceSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            edible: Option<crate::EdibleSpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            supply: Option<crate::content_ids::SupplyId>,
            class: ItemClass,
            archetype: String,
            identity: String,
            name: String,
            appearance: String,
            concealed: bool,
            stackable: bool,
            properties: BTreeMap<String, String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            magical: fields.magical,
            held_light: fields.held_light,
            currency: fields.currency,
            equipment: fields.equipment,
            projectile: fields.projectile,
            equipment_generation: fields.equipment_generation,
            consumable: fields.consumable,
            readable: fields.readable,
            ground_scare: fields.ground_scare,
            device: fields.device,
            edible: fields.edible,
            supply: fields.supply,
            class: fields.class,
            archetype: fields.archetype,
            identity: fields.identity,
            name: fields.name,
            appearance: fields.appearance,
            concealed: fields.concealed,
            stackable: fields.stackable,
            properties: fields.properties,
            asset: fields.asset,
        })
    }
}

#[cfg(test)]
thread_local! {
    static ITEM_DEFINITION_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Clone for ItemSpec {
    fn clone(&self) -> Self {
        ITEM_DEFINITION_COPIES.with(|copies| copies.set(copies.get() + 1));
        Self {
            magical: self.magical,
            held_light: self.held_light,
            currency: self.currency,
            equipment: self.equipment.clone(),
            projectile: self.projectile.clone(),
            equipment_generation: self.equipment_generation,
            consumable: self.consumable.clone(),
            readable: self.readable.clone(),
            ground_scare: self.ground_scare.clone(),
            device: self.device.clone(),
            edible: self.edible.clone(),
            supply: self.supply.clone(),
            class: self.class,
            archetype: self.archetype.clone(),
            identity: self.identity.clone(),
            name: self.name.clone(),
            appearance: self.appearance.clone(),
            concealed: self.concealed,
            stackable: self.stackable,
            properties: self.properties.clone(),
            asset: self.asset.clone(),
        }
    }
}

impl ItemSpec {
    /// Authored capability, never observer knowledge, controls instance provenance.
    pub(crate) fn has_instance_condition(&self) -> bool {
        self.equipment.is_some() || self.projectile.is_some() || self.ground_scare.is_some()
    }

    pub fn ordinary(name: String) -> Self {
        Self {
            magical: false,
            held_light: None,
            currency: None,
            equipment: None,
            projectile: None,
            equipment_generation: None,
            consumable: None,
            readable: None,
            ground_scare: None,
            device: None,
            edible: None,
            supply: None,
            class: ItemClass::Misc,
            archetype: name.clone(),
            identity: name.clone(),
            appearance: name.clone(),
            name,
            concealed: false,
            stackable: false,
            properties: BTreeMap::new(),
            asset: None,
        }
    }
    /// Validate every content reference, including unequipped and ground items.
    pub fn validate_rules(
        &self,
        rules: &crate::rules::Ruleset,
    ) -> Result<(), crate::rules::RulesetError> {
        if self.equipment_generation.is_some_and(|policy| {
            !policy.valid_for(
                self.class,
                self.equipment.as_ref(),
                self.projectile.as_ref(),
            )
        }) {
            return Err(crate::rules::RulesetError::Bound {
                field: "equipment_generation".into(),
            });
        }
        if let Some(supply) = &self.supply {
            if self.equipment.is_some() {
                return Err(crate::rules::RulesetError::Bound {
                    field: "equipment_supply".into(),
                });
            }
            if rules.supply(supply).is_none() {
                return Err(crate::rules::RulesetError::UnknownReference {
                    category: "supply",
                    id: supply.to_string(),
                });
            }
        }
        if let Some(equipment) = &self.equipment {
            equipment.validate_rules(self.class, rules)?;
        }
        if let Some(projectile) = &self.projectile {
            if self.class != ItemClass::Weapon {
                return Err(crate::rules::RulesetError::Bound {
                    field: "projectile_class".into(),
                });
            }
            projectile
                .validate_shape()
                .map_err(|_| crate::rules::RulesetError::Bound {
                    field: "projectile".into(),
                })?;
            rules.validate_damage(&projectile.damage)?;
        }
        if let Some(consumable) = &self.consumable {
            rules.validate_effects(&consumable.effects)?;
            rules.validate_payload_recipients(
                &consumable.descriptors,
                consumable
                    .effects
                    .iter()
                    .map(crate::rules::Effect::recipient),
            )?;
        }
        if self
            .ground_scare
            .as_ref()
            .is_some_and(|spec| !spec.valid(rules))
        {
            return Err(crate::rules::RulesetError::Bound {
                field: "ground_scare".into(),
            });
        }
        if let Some(readable) = &self.readable {
            readable.validate_rules(rules)?;
        }
        if let Some(device) = &self.device {
            device.validate_rules(rules)?;
        }
        if let Some(edible) = &self.edible {
            if !edible.valid() {
                return Err(crate::rules::RulesetError::Bound {
                    field: "edible".into(),
                });
            }
            rules.validate_effects(&edible.effects())?;
        }
        Ok(())
    }
    pub(crate) fn valid(&self) -> bool {
        self.magic_shape_valid()
            && self.held_light.is_none_or(|policy| {
                policy.valid() && self.class == ItemClass::Tool && !self.stackable
                    && self.equipment.as_ref().is_some_and(|equipment|
                        equipment.slot.as_str() == "held_light" && equipment.attack.is_none())
                    && self.device.is_none() && self.consumable.is_none()
                    && self.readable.is_none() && self.edible.is_none()
                    && self.projectile.is_none() && self.supply.is_none()
            })
            && self.equipment_generation.is_none_or(|policy| {
                policy.valid_for(
                    self.class,
                    self.equipment.as_ref(),
                    self.projectile.as_ref(),
                )
            })
            && self.currency.is_none_or(|_| {
                self.class == ItemClass::Coin
                    && self.stackable
                    && self.equipment.is_none()
                    && self.projectile.is_none()
                    && self.consumable.is_none()
                    && self.edible.is_none()
                    && self.supply.is_none()
            })
            && self.equipment.as_ref().is_none_or(|equipment| {
                !self.stackable
                    && self.consumable.is_none()
                    && self.edible.is_none()
                    && self.supply.is_none()
                    && equipment.valid_shape()
            })
            && self.projectile.as_ref().is_none_or(|projectile| {
                self.class == ItemClass::Weapon && projectile.validate_shape().is_ok()
            })
            && self.consumable.as_ref().is_none_or(|consumable| {
                self.class == ItemClass::Potion && self.edible.is_none() && consumable.valid()
            })
            && self.edible.as_ref().is_none_or(|edible| {
                edible.valid() && self.equipment.is_none() && self.consumable.is_none()
            })
            && [
                &self.archetype,
                &self.identity,
                &self.name,
                &self.appearance,
            ]
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control))
            && self.properties.len() <= 32
            && self.properties.iter().all(|(k, v)| {
                !k.is_empty()
                    && k.len() <= 80
                    && v.len() <= 80
                    && !k.chars().chain(v.chars()).any(char::is_control)
            })
    }
}

impl Game {
    fn item_is_equipped(&self, id: ItemId) -> bool {
        self.items.get(&id).is_some_and(|item| match item.location {
            ItemLocation::Carried(owner) => self
                .actors
                .get(&owner)
                .is_some_and(|actor| actor.equipment.values().any(|equipped| *equipped == id)),
            ItemLocation::Ground(_) => false,
        })
    }
    pub(crate) fn merge_destination_available(&self, id: ItemId) -> bool {
        self.items.get(&id).is_some_and(|item| {
            !item.spec.has_instance_condition()
                || (item.holds.is_empty() && !self.item_is_equipped(id))
        })
    }
    pub(crate) fn prepare_stack_transfer(
        &self,
        source: ItemId,
        merge: Option<ItemId>,
        quantity: u64,
        destination: ItemLocation,
        knowledge_owner: Option<ActorId>,
    ) -> Result<PreparedTransfer, GameError> {
        let condition = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .condition;
        self.prepare_stack_transfer_with_condition(
            source,
            merge,
            quantity,
            destination,
            knowledge_owner,
            condition,
        )
    }
    fn prepare_stack_transfer_with_condition(
        &self,
        source: ItemId,
        merge: Option<ItemId>,
        quantity: u64,
        destination: ItemLocation,
        knowledge_owner: Option<ActorId>,
        condition_after: crate::armor::ItemCondition,
    ) -> Result<PreparedTransfer, GameError> {
        let item = self.items.get(&source).ok_or(GameError::ItemUnavailable)?;
        let mut transferred = item.clone();
        transferred.condition = condition_after;
        if transferred.condition != item.condition && !transferred.condition_valid() {
            return Err(GameError::ItemUnavailable);
        }
        if quantity == 0 || quantity > item.quantity {
            return Err(GameError::InvalidQuantity);
        }
        if item.free_quantity(None).is_none_or(|free| free < quantity)
            || self.item_is_equipped(source)
        {
            return Err(GameError::ItemUnavailable);
        }
        if let Some(id) = merge {
            let existing = self
                .items
                .get(&id)
                .filter(|existing| {
                    id != source
                        && existing.location == destination
                        && existing.stacks_with(&transferred)
                })
                .ok_or(GameError::ItemUnavailable)?;
            if self.merge_destination_available(id) {
                existing
                    .quantity
                    .checked_add(quantity)
                    .ok_or(GameError::InvalidQuantity)?;
            }
        }
        // A held destination keeps its exact claims; use a separate stack.
        let merge = merge.filter(|id| self.merge_destination_available(*id));
        let result = match merge {
            Some(id) if !item.spec.has_instance_condition() => id,
            None if quantity == item.quantity => source,
            _ => {
                self.next_item_id
                    .checked_add(1)
                    .ok_or(GameError::IdentityExhausted)?;
                ItemId(self.next_item_id)
            }
        };
        let inherited = knowledge_owner.and_then(|owner| {
            item.spec.has_instance_condition().then_some(())?;
            let facts = &self.actors.get(&owner)?.known_equipment;
            facts
                .inherited_transfer(source, merge, result)
                .map(|facts| (owner, facts))
        });
        Ok(PreparedTransfer {
            source,
            expected_source: item.clone(),
            result,
            merge,
            expected_destination: merge.map(|id| self.items[&id].clone()),
            inherited,
            destination,
            quantity,
            expected_next_item_id: self.next_item_id,
            condition_after,
            crumble: false,
        })
    }
    pub(crate) fn apply_prepared_transfer(
        &mut self,
        plan: &PreparedTransfer,
    ) -> Result<(), GameError> {
        if self.next_item_id != plan.expected_next_item_id
            || self.items.get(&plan.source) != Some(&plan.expected_source)
            || plan
                .merge
                .is_some_and(|id| self.items.get(&id) != plan.expected_destination.as_ref())
            || self.item_is_equipped(plan.source)
        {
            return Err(GameError::ItemUnavailable);
        }
        if plan.crumble {
            if plan.expected_source.spec.ground_scare.is_none()
                || !plan.expected_source.condition.cursed
                || !matches!(plan.expected_source.location, ItemLocation::Ground(_))
                || !matches!(plan.destination, ItemLocation::Carried(_))
                || plan.quantity == 0
                || plan
                    .expected_source
                    .free_quantity(None)
                    .is_none_or(|free| free < plan.quantity)
            {
                return Err(GameError::ItemUnavailable);
            }
            let remainder = plan
                .expected_source
                .quantity
                .checked_sub(plan.quantity)
                .ok_or(GameError::InvalidQuantity)?;
            if remainder == 0 {
                self.items.remove(&plan.source);
            } else {
                self.items
                    .edit(plan.source, |item| item.quantity = remainder)
                    .ok_or(GameError::ItemUnavailable)?;
            }
            return Ok(());
        }
        self.apply_transfer_with_condition(
            plan.source,
            plan.result,
            plan.merge,
            plan.quantity,
            plan.destination,
            plan.condition_after,
        )?;
        if let Some((owner, facts)) = &plan.inherited {
            if let Some(mut owner) = self.actors.get_mut(owner) {
                owner.known_equipment.remember_inherited(facts.clone());
            }
        }
        if plan.condition_after != plan.expected_source.condition {
            if let ItemLocation::Carried(owner) = plan.destination {
                self.actors
                    .get_mut(&owner)
                    .ok_or(GameError::UnknownActor)?
                    .known_equipment
                    .observe_change(
                        plan.result,
                        plan.expected_source.condition,
                        plan.condition_after,
                    );
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_owned_transfer(
        &self,
        source: ItemId,
        owner: ActorId,
        recipient: ActorId,
        quantity: u64,
    ) -> Result<PreparedTransfer, GameError> {
        self.prepare_owned_transfer_selecting(
            source,
            owner,
            recipient,
            quantity,
            |game, item, at| {
                item.spec
                    .stackable
                    .then(|| {
                        game.items.at(at).find(|id| {
                            crate::diagnostics::stack_candidate();
                            game.items[id].stacks_with(item)
                                && game.merge_destination_available(*id)
                        })
                    })
                    .flatten()
            },
        )
    }

    /// A candidate-local index may select the merge identity, but it cannot
    /// bypass the same full-spec, ownership, quantity and exact-witness checks.
    pub(crate) fn prepare_owned_transfer_into(
        &self,
        source: ItemId,
        owner: ActorId,
        recipient: ActorId,
        quantity: u64,
        merge: Option<ItemId>,
    ) -> Result<PreparedTransfer, GameError> {
        self.prepare_owned_transfer_selecting(source, owner, recipient, quantity, |_, _, _| merge)
    }

    fn prepare_owned_transfer_selecting(
        &self,
        source: ItemId,
        owner: ActorId,
        recipient: ActorId,
        quantity: u64,
        select: impl FnOnce(&Self, &Item, ItemLocation) -> Option<ItemId>,
    ) -> Result<PreparedTransfer, GameError> {
        if owner == recipient {
            return Err(GameError::ItemUnavailable);
        }
        let actor = self.actors.get(&owner).ok_or(GameError::UnknownActor)?;
        if !actor.alive()
            || self
                .actors
                .get(&recipient)
                .is_none_or(|actor| !actor.alive())
            || actor.equipment.values().any(|equipped| *equipped == source)
        {
            return Err(GameError::ItemUnavailable);
        }
        let item = self
            .items
            .get(&source)
            .filter(|item| item.location == ItemLocation::Carried(owner))
            .ok_or(GameError::ItemUnavailable)?;
        if quantity == 0 || quantity > item.quantity {
            return Err(GameError::InvalidQuantity);
        }
        if item.free_quantity(None).is_none_or(|free| free < quantity) {
            return Err(GameError::ItemUnavailable);
        }
        let destination = ItemLocation::Carried(recipient);
        let merge = select(self, item, destination);
        self.prepare_stack_transfer(source, merge, quantity, destination, None)
    }

    pub(crate) fn apply_owned_transfer(&mut self, plan: PreparedTransfer) -> Result<(), GameError> {
        if !matches!(plan.expected_source.location, ItemLocation::Carried(_)) {
            return Err(GameError::ItemUnavailable);
        }
        self.apply_prepared_transfer(&plan)
    }
    pub fn place_item_stack(
        &mut self,
        id: u64,
        at: Location,
        owner: Option<ActorId>,
        quantity: u64,
        spec: ItemSpec,
    ) -> Result<ItemId, GameError> {
        if quantity == 0 || (!spec.stackable && quantity != 1) || !spec.valid() {
            return Err(GameError::InvalidQuantity);
        }
        spec.validate_rules(self.content.rules())
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidDefinition))?;
        let mut rng = self.rules_rng;
        let condition = spec
            .equipment_generation
            .map(|policy| {
                policy.sample(
                    spec.class,
                    spec.equipment.as_ref(),
                    spec.projectile.as_ref(),
                    &mut rng,
                )
            })
            .transpose()?
            .unwrap_or_default();
        let charges = spec
            .device
            .as_ref()
            .map(|device| device.initial_charges.sample(&mut rng))
            .transpose()?;
        let item = self.place_authored_item(id, at, spec.name.clone(), owner)?;
        self.items
            .edit(item, |entry| {
                entry.relocation_elapsed = spec
                    .equipment
                    .as_ref()
                    .and_then(|gear| gear.periodic_relocation)
                    .map(|_| 0);
                entry.spec = tor_world::Shared::new(spec);
                entry.quantity = quantity;
                entry.charges = charges;
                entry.condition = condition;
            })
            .expect("placed item");
        self.rules_rng = rng;
        Ok(item)
    }

    /// Scenario/privileged operation; knowledge outlives all instances of an item.
    pub fn learn_identity(&mut self, actor: ActorId, identity: &str) -> Result<(), GameError> {
        if identity.is_empty() || identity.len() > 80 || identity.chars().any(char::is_control) {
            return Err(GameError::ItemUnavailable);
        }
        self.actors
            .get_mut(&actor)
            .ok_or(GameError::UnknownActor)?
            .knowledge
            .insert(identity.into());
        Ok(())
    }

    pub fn identify_item(&mut self, actor: ActorId, item: ItemId) -> Result<(), GameError> {
        let entry = self.items.get(&item).ok_or(GameError::ItemUnavailable)?;
        let identity = entry.spec.identity.clone();
        let condition = entry
            .spec
            .has_instance_condition()
            .then_some(entry.condition);
        let mut owner = self.actors.get_mut(&actor).ok_or(GameError::UnknownActor)?;
        if let Some(condition) = condition {
            owner.known_equipment.identify(item, condition)?;
        }
        owner.knowledge.insert(identity);
        Ok(())
    }

    /// Place one existing objective item without allocation or transfer semantics.
    /// The enclosing entry candidate owns rollback and eventual publication.
    pub(crate) fn relocate_exact_item_in_candidate(
        &mut self,
        id: ItemId,
        expected_source: ItemLocation,
        destination: Location,
    ) -> Result<(), GameError> {
        let item = self.items.get(&id).ok_or(GameError::ItemUnavailable)?;
        if item.location != expected_source
            || !self.item_state_valid(id, item)
            || item.quantity != 1
            || item.spec.class != ItemClass::Amulet
            || item.spec.stackable
            || item.spec.equipment.is_some()
            || item.spec.consumable.is_some()
            || item.spec.edible.is_some()
            || item.spec.readable.is_some()
            || item.spec.device.is_some()
            || item.spec.supply.is_some()
            // An item without payment capabilities cannot own valid holds.
            // Reject corrupt evidence rather than silently releasing it.
            || !item.holds.is_empty()
            || self
                .actors
                .values()
                .any(|actor| actor.equipment.values().any(|equipped| *equipped == id))
        {
            return Err(GameError::ItemUnavailable);
        }
        if !self.world.walkable(destination) {
            return Err(GameError::InvalidLocation);
        }
        let location = ItemLocation::Ground(destination);
        if expected_source != location {
            self.invalidate_item_claimants(id)?;
        }
        self.items
            .edit(id, |item| {
                item.location = location;
                item.motion = crate::MotionState::default();
                item.orientation = 0;
            })
            .ok_or(GameError::ItemUnavailable)
    }

    pub(crate) fn prepare_transfer(
        &self,
        actor: ActorId,
        item: ItemId,
        requested: Option<u64>,
        taking: bool,
    ) -> Result<PreparedTransfer, GameError> {
        // Deny ground reach before inspecting an exact item or its quantity.
        if taking && self.actors[&actor].locomotion().levitating {
            return Err(GameError::ItemUnavailable);
        }
        let at = self.actors[&actor].location;
        if !taking
            && self.actors[&actor]
                .equipment
                .values()
                .any(|equipped| *equipped == item)
        {
            return Err(GameError::ItemUnavailable);
        }
        let (from, to) = if taking {
            (ItemLocation::Ground(at), ItemLocation::Carried(actor))
        } else {
            (ItemLocation::Carried(actor), ItemLocation::Ground(at))
        };
        let source = self
            .items
            .get(&item)
            .filter(|i| i.location == from)
            .ok_or(GameError::ItemUnavailable)?;
        let quantity = requested.unwrap_or(source.quantity);
        if quantity == 0 || quantity > source.quantity {
            return Err(GameError::InvalidQuantity);
        }
        if source
            .free_quantity(None)
            .is_none_or(|free| quantity > free)
        {
            return Err(GameError::ItemUnavailable);
        }
        let scare_pickup = taking && source.spec.ground_scare.is_some();
        if scare_pickup && source.condition.cursed {
            return Ok(PreparedTransfer {
                source: item,
                expected_source: source.clone(),
                result: item,
                merge: None,
                expected_destination: None,
                inherited: None,
                destination: to,
                quantity,
                expected_next_item_id: self.next_item_id,
                condition_after: source.condition,
                crumble: true,
            });
        }
        let mut transferred = source.clone();
        if scare_pickup {
            transferred.condition.cursed = true;
        }
        let destination = if source.spec.stackable {
            self.items
                .at(to)
                .map(|id| (id, &self.items[&id]))
                .find(|(_id, i)| {
                    crate::diagnostics::stack_candidate();
                    i.stacks_with(&transferred)
                        && self.merge_destination_available(*_id)
                        && (taking
                            || (i.motion.same_kinematics(&self.actors[&actor].motion)
                                && i.orientation == self.actors[&actor].orientation)
                            || (i.motion == crate::MotionState::default()
                                && self.actors[&actor]
                                    .motion
                                    .same_kinematics(&crate::MotionState::default())))
                })
        } else {
            None
        };
        self.prepare_stack_transfer_with_condition(
            item,
            destination.map(|(id, _)| id),
            quantity,
            to,
            Some(actor),
            transferred.condition,
        )
    }

    #[cfg(test)]
    pub(crate) fn apply_transfer(
        &mut self,
        source: ItemId,
        result: ItemId,
        quantity: u64,
        location: ItemLocation,
    ) -> Result<(), GameError> {
        let merge = (source != result && self.items.contains_key(&result)).then_some(result);
        self.apply_transfer_with_merge(source, result, merge, quantity, location)
    }

    #[cfg(test)]
    fn apply_transfer_with_merge(
        &mut self,
        source: ItemId,
        result: ItemId,
        merge: Option<ItemId>,
        quantity: u64,
        location: ItemLocation,
    ) -> Result<(), GameError> {
        let condition = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .condition;
        self.apply_transfer_with_condition(source, result, merge, quantity, location, condition)
    }
    fn apply_transfer_with_condition(
        &mut self,
        source: ItemId,
        result: ItemId,
        merge: Option<ItemId>,
        quantity: u64,
        location: ItemLocation,
        condition: crate::armor::ItemCondition,
    ) -> Result<(), GameError> {
        let item = self.items.get(&source).ok_or(GameError::ItemUnavailable)?;
        let mut transferred = item.clone();
        transferred.condition = condition;
        if transferred.condition != item.condition && !transferred.condition_valid() {
            return Err(GameError::ItemUnavailable);
        }
        if quantity == 0
            || quantity > item.quantity
            || (source == result && quantity != item.quantity)
        {
            return Err(GameError::InvalidQuantity);
        }
        if item.free_quantity(None).is_none_or(|free| quantity > free) {
            return Err(GameError::ItemUnavailable);
        }
        let destination_quantity = merge
            .map(|id| {
                let destination = self.items.get(&id).ok_or(GameError::ItemUnavailable)?;
                if id == source
                    || destination.location != location
                    || !transferred.stacks_with(destination)
                    || !self.merge_destination_available(id)
                    || (item.spec.has_instance_condition() && result == id)
                    || (!item.spec.has_instance_condition() && result != id)
                {
                    return Err(GameError::ItemUnavailable);
                }
                destination
                    .quantity
                    .checked_add(quantity)
                    .ok_or(GameError::InvalidQuantity)
            })
            .transpose()?;
        if source == result && merge.is_some() {
            return Err(GameError::ItemUnavailable);
        }
        let fresh = source != result && merge != Some(result);
        let next_item_id = if fresh {
            if result != ItemId(self.next_item_id) || self.items.contains_key(&result) {
                return Err(GameError::ItemUnavailable);
            }
            self.next_item_id
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?
        } else {
            self.next_item_id
        };
        let (motion, orientation) = match (
            self.items
                .get(&source)
                .ok_or(GameError::ItemUnavailable)?
                .location,
            location,
        ) {
            (ItemLocation::Carried(owner), ItemLocation::Ground(_)) => (
                self.actors
                    .get(&owner)
                    .ok_or(GameError::UnknownActor)?
                    .motion
                    .without_fall(),
                self.actors
                    .get(&owner)
                    .ok_or(GameError::UnknownActor)?
                    .orientation,
            ),
            _ => (crate::MotionState::default(), 0),
        };
        if source == result {
            self.items
                .edit(source, |item| {
                    item.motion = motion;
                    item.orientation = orientation;
                    item.location = location;
                    item.condition = condition;
                })
                .ok_or(GameError::ItemUnavailable)?;
            return Ok(());
        }
        let charges = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .charges;
        let relocation_elapsed = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .relocation_elapsed;
        let spec = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .spec
            .clone();
        let remainder = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .quantity
            .checked_sub(quantity)
            .ok_or(GameError::InvalidQuantity)?;
        if remainder == 0 {
            self.items.remove(&source);
        } else {
            self.items
                .edit(source, |item| item.quantity = remainder)
                .ok_or(GameError::ItemUnavailable)?;
        }
        if merge == Some(result) {
            self.items
                .edit(result, |existing| {
                    existing.quantity = destination_quantity.expect("validated merge")
                })
                .ok_or(GameError::ItemUnavailable)?;
        } else {
            if let Some(merge) = merge {
                self.items.remove(&merge);
            }
            self.items.insert(
                result,
                Item {
                    condition,
                    charges,
                    relocation_elapsed,
                    corpse: None,
                    holds: vec![],
                    motion,
                    orientation,
                    spec,
                    quantity: destination_quantity.unwrap_or(quantity),
                    location,
                },
            );
            self.next_item_id = next_item_id;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Position, RegionId};

    fn relocation_at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }

    fn relocation_fixture(owner: Option<ActorId>) -> (Game, ItemId) {
        let mut game = Game::region_corridor(42, 2, crate::test_creatures::content());
        if let Some(owner) = owner {
            game.spawn_authored_actor(owner.0, relocation_at(1, 1), NonZeroU64::new(100).unwrap())
                .unwrap();
        }
        let mut spec = ItemSpec::ordinary("unique amulet".into());
        spec.class = ItemClass::Amulet;
        spec.properties.insert("material".into(), "gold".into());
        let item = game
            .place_item_stack(100, relocation_at(1, 1), owner, 1, spec)
            .unwrap();
        (game, item)
    }

    #[test]
    fn exact_relocation_moves_ground_item_without_recreating_or_merging() {
        let (mut game, item) = relocation_fixture(None);
        let source = ItemLocation::Ground(relocation_at(1, 1));
        let destination = relocation_at(2, 2);
        let spec = game.items[&item].spec.clone();
        let pile = game
            .place_item_stack(101, destination, None, 1, (*spec).clone())
            .unwrap();
        let actor = game
            .spawn_actor(destination, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.items.edit(item, |entry| {
            entry.motion.velocity = [17, 0, 0];
            entry.motion.displacement = [0, 7, 0];
            entry.orientation = 7;
        });
        let allocator = game.next_item_id;
        let before = game.clone();

        game.relocate_exact_item_in_candidate(item, source, destination)
            .unwrap();

        assert_eq!(game.items.len(), before.items.len());
        assert_eq!(game.items[&item].spec, spec);
        assert!(game.items[&item]
            .spec
            .shares_storage(&before.items[&item].spec));
        assert_eq!(game.items[&item].quantity, 1);
        assert_eq!(
            game.items[&item].location,
            ItemLocation::Ground(destination)
        );
        assert_eq!(game.items[&item].motion, crate::MotionState::default());
        assert_eq!(game.items[&item].orientation, 0);
        assert_eq!(game.items.at(source).count(), 0);
        assert_eq!(
            game.items
                .at(ItemLocation::Ground(destination))
                .collect::<Vec<_>>(),
            vec![item, pile]
        );
        assert_eq!(game.next_item_id, allocator);
        assert_eq!(game.actors[&actor], before.actors[&actor]);
        assert_eq!(game.tick(), before.tick());
        assert_eq!(game.rules_rng, before.rules_rng);
    }

    #[test]
    fn exact_relocation_removes_npc_inventory_without_inheriting_motion() {
        let carrier = ActorId(7);
        let (mut game, item) = relocation_fixture(Some(carrier));
        let source = ItemLocation::Carried(carrier);
        let destination = relocation_at(2, 3);
        let mut actor = game.actors.get_mut(&carrier).unwrap();
        actor.motion.velocity = [19, 0, 0];
        actor.motion.displacement = [0, 11, 0];
        actor.orientation = 8;
        drop(actor);
        let before = game.clone();

        game.relocate_exact_item_in_candidate(item, source, destination)
            .unwrap();

        assert_eq!(game.items.at(source).count(), 0);
        assert_eq!(
            game.items[&item].location,
            ItemLocation::Ground(destination)
        );
        assert_eq!(game.items[&item].motion, crate::MotionState::default());
        assert_eq!(game.items[&item].orientation, 0);
        assert_eq!(game.items[&item].spec, before.items[&item].spec);
        assert_eq!(game.items[&item].quantity, before.items[&item].quantity);
        assert_eq!(game.next_item_id, before.next_item_id);
        assert_eq!(game.actors[&carrier], before.actors[&carrier]);
    }

    #[test]
    fn exact_relocation_same_cell_is_a_resting_placement() {
        let (mut game, item) = relocation_fixture(None);
        let at = relocation_at(1, 1);
        game.items.edit(item, |entry| {
            entry.motion.velocity = [31, 0, 0];
            entry.motion.displacement = [0, 13, 0];
            entry.orientation = 9;
        });
        let before = game.clone();

        game.relocate_exact_item_in_candidate(item, ItemLocation::Ground(at), at)
            .unwrap();

        assert_eq!(
            game.items.at(ItemLocation::Ground(at)).collect::<Vec<_>>(),
            vec![item]
        );
        assert_eq!(game.items[&item].motion, crate::MotionState::default());
        assert_eq!(game.items[&item].orientation, 0);
        assert_eq!(game.items[&item].spec, before.items[&item].spec);
        assert_eq!(game.next_item_id, before.next_item_id);
        assert_eq!(game.items.len(), 1);
    }

    #[test]
    fn exact_relocation_rejects_missing_item_and_stale_expected_source() {
        let (mut game, item) = relocation_fixture(None);
        for (id, source) in [
            (ItemId(999), ItemLocation::Ground(relocation_at(1, 1))),
            (item, ItemLocation::Ground(relocation_at(1, 2))),
            (item, ItemLocation::Carried(ActorId(7))),
        ] {
            let before = game.clone();
            assert_eq!(
                game.relocate_exact_item_in_candidate(id, source, relocation_at(2, 2)),
                Err(GameError::ItemUnavailable)
            );
            assert_eq!(game, before);
        }
    }

    #[test]
    fn exact_relocation_rejects_invalid_world_destination_before_mutation() {
        let (mut game, item) = relocation_fixture(None);
        let wall = relocation_at(2, 4);
        game.set_wall(wall, true).unwrap();
        for destination in [wall, relocation_at(2, 12), relocation_at(999, 1)] {
            let before = game.clone();
            assert_eq!(
                game.relocate_exact_item_in_candidate(
                    item,
                    ItemLocation::Ground(relocation_at(1, 1)),
                    destination,
                ),
                Err(GameError::InvalidLocation)
            );
            assert_eq!(game, before);
        }
    }

    #[test]
    fn exact_relocation_rejects_non_artifact_shapes_and_capabilities() {
        let (original, item) = relocation_fixture(None);
        let source = original.items[&item].location;
        for corruption in 0..9 {
            let mut game = original.clone();
            game.items.edit(item, |entry| match corruption {
                0 => entry.quantity = 0,
                1 => entry.quantity = 2,
                2 => entry.spec.stackable = true,
                3 => entry.spec.class = ItemClass::Misc,
                4 => entry.spec.identity.clear(),
                5 => {
                    entry.spec.equipment = Some(crate::EquipmentSpec {
                        periodic_relocation: None,
                        potency_modifiers: Vec::new(),
                        weapon_profile: None,
                        armor: None,
                        slot: crate::content_ids::EquipmentSlot::new("shield").unwrap(),
                        attack: None,
                        modifiers: vec![],
                    })
                }
                6 => {
                    entry.spec.consumable = Some(crate::ConsumableSpec {
                        descriptors: vec![],
                        discovery: crate::ItemDiscovery::SelfEffect,
                        effects: vec![],
                    })
                }
                7 => {
                    entry.spec.edible = Some(crate::EdibleSpec {
                        nutrition: crate::rules::AmountExpr::Fixed { amount: 1 },
                    })
                }
                8 => {
                    entry.spec.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap())
                }
                _ => unreachable!(),
            });
            let before = game.clone();
            assert_eq!(
                game.relocate_exact_item_in_candidate(item, source, relocation_at(2, 2)),
                Err(GameError::ItemUnavailable),
                "corruption {corruption}"
            );
            assert_eq!(game, before, "corruption {corruption}");
        }
    }

    #[test]
    fn exact_relocation_rejects_missing_carrier_and_equipped_aliases() {
        let carrier = ActorId(7);
        let (original, item) = relocation_fixture(Some(carrier));
        for corruption in 0..3 {
            let mut game = original.clone();
            match corruption {
                0 => {
                    game.items.edit(item, |entry| {
                        entry.location = ItemLocation::Carried(ActorId(999));
                    });
                }
                1 => {
                    game.actors
                        .get_mut(&carrier)
                        .unwrap()
                        .equipment
                        .insert(crate::EquipmentSlotId(0), item);
                }
                2 => {
                    let other = game
                        .spawn_actor(relocation_at(1, 2), NonZeroU64::new(100).unwrap())
                        .unwrap();
                    game.actors
                        .get_mut(&other)
                        .unwrap()
                        .equipment
                        .insert(crate::EquipmentSlotId(0), item);
                }
                _ => unreachable!(),
            }
            let source = game.items[&item].location;
            let before = game.clone();
            assert_eq!(
                game.relocate_exact_item_in_candidate(item, source, relocation_at(2, 2)),
                Err(GameError::ItemUnavailable)
            );
            assert_eq!(game, before);
        }
    }

    #[test]
    fn exact_relocation_rejects_claimants_instead_of_erasing_invalid_evidence() {
        let (original, item) = relocation_fixture(Some(ActorId(7)));
        for quantity in [0, 1, 2] {
            let mut game = original.clone();
            game.items.edit(item, |entry| {
                entry.holds.push(crate::supplies::ItemHold {
                    key: crate::supplies::ItemClaimKey {
                        lineage: crate::supplies::PreparationLineage {
                            actor: ActorId(7),
                            origin: crate::IntentionId(1),
                        },
                        cost_ordinal: 0,
                    },
                    quantity,
                });
            });
            let before = game.clone();
            assert_eq!(
                game.relocate_exact_item_in_candidate(
                    item,
                    ItemLocation::Carried(ActorId(7)),
                    relocation_at(2, 2),
                ),
                Err(GameError::ItemUnavailable)
            );
            assert_eq!(game, before);
            assert_eq!(
                game.items.claimed().map(|(id, _)| id).collect::<Vec<_>>(),
                vec![item]
            );
        }
    }

    #[test]
    fn exact_relocation_parent_candidate_discards_a_later_failure() {
        let (original, item) = relocation_fixture(None);
        let before = original.clone();
        let source = original.items[&item].location;
        let destination = relocation_at(2, 2);
        let mut candidate = original.clone();
        let result = (|| {
            candidate.relocate_exact_item_in_candidate(item, source, destination)?;
            candidate.relocate_exact_item_in_candidate(item, source, relocation_at(2, 3))
        })();
        assert_eq!(result, Err(GameError::ItemUnavailable));
        assert_ne!(
            candidate.items[&item].location,
            original.items[&item].location
        );
        drop(candidate);
        assert_eq!(original, before);

        let mut retried = original.clone();
        retried
            .relocate_exact_item_in_candidate(item, source, destination)
            .unwrap();
        let mut control = before.clone();
        control
            .relocate_exact_item_in_candidate(item, source, destination)
            .unwrap();
        assert_eq!(retried, control);
        assert_eq!(retried.next_item_id, original.next_item_id);
    }

    #[test]
    fn supplies_are_part_of_stack_identity_and_require_current_owned_field() {
        let mut value = serde_json::to_value(ItemSpec::ordinary("bandage".into())).unwrap();
        value["supply"] = serde_json::json!("bandage");
        value["stackable"] = serde_json::json!(true);
        let bandage: ItemSpec = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(bandage, bandage.clone());
        let rules = crate::test_creatures::content();
        assert!(bandage.validate_rules(rules.rules()).is_ok());
        value["supply"] = serde_json::json!("ration");
        let ration: ItemSpec = serde_json::from_value(value.clone()).unwrap();
        assert_ne!(bandage, ration);
        value["supply"] = serde_json::json!("unknown_supply");
        let unknown: ItemSpec = serde_json::from_value(value.clone()).unwrap();
        assert!(unknown.validate_rules(rules.rules()).is_err());
        value.as_object_mut().unwrap().remove("supply");
        assert!(serde_json::from_value::<ItemSpec>(value).is_err());
    }

    #[test]
    fn equipment_cannot_also_be_finite_supply_content() {
        let rules = crate::test_creatures::content();
        let mut value = serde_json::to_value(ItemSpec::ordinary("bandage shield".into())).unwrap();
        value["class"] = serde_json::json!("armor");
        value["supply"] = serde_json::json!("bandage");
        value["equipment"] = serde_json::json!({"slot":"shield", "periodic_relocation":null, "armor":null, "attack":null, "weapon_profile":null, "modifiers":[], "potency_modifiers":[]});
        let spec: ItemSpec = serde_json::from_value(value).unwrap();
        assert!(!spec.valid());
        assert!(spec.validate_rules(rules.rules()).is_err());
    }

    #[test]
    fn stack_splits_do_not_copy_unrelated_item_definitions() {
        for count in [16, 256, 4096] {
            let mut game = Game::new(
                (*Game::two_room(42, crate::test_creatures::content()).world).clone(),
                42,
                crate::test_creatures::content(),
            );
            let at = Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            };
            let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
            for id in 1..=count {
                let mut spec = ItemSpec::ordinary(format!("token-{id}"));
                spec.stackable = true;
                spec.properties.insert("material".into(), "stone".into());
                game.place_item_stack(id, at, None, 2, spec).unwrap();
            }
            let old = game.clone();
            let before = ITEM_DEFINITION_COPIES.with(|copies| copies.get());
            game.act(
                actor,
                crate::Action::Take {
                    item: ItemId(1),
                    quantity: Some(1),
                },
            )
            .unwrap();
            let copied = ITEM_DEFINITION_COPIES.with(|copies| copies.get()) - before;
            assert_eq!(copied, 0, "item count {count}");
            assert_eq!(old.items.len(), count as usize);
            assert_eq!(old.items[&ItemId(1)].quantity, 2);
            assert_eq!(game.items[&ItemId(1)].quantity, 1);
            let taken = ItemId(count + 1);
            assert_eq!(game.items[&taken].quantity, 1);
            assert_eq!(game.items[&taken].location, ItemLocation::Carried(actor));
            assert_eq!(game.items[&taken].spec, old.items[&ItemId(1)].spec);
        }
    }

    #[test]
    fn knowledge_survives_removal_of_last_instance_without_item_use_mechanics() {
        let mut game = Game::two_room(0, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        let mut spec = ItemSpec::ordinary("healing".into());
        spec.appearance = "red potion".into();
        spec.concealed = true;
        game.place_item_stack(100, at, None, 1, spec.clone())
            .unwrap();
        game.identify_item(actor, ItemId(100)).unwrap();
        game.items.remove(&ItemId(100));
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        restored.place_item_stack(101, at, None, 1, spec).unwrap();
        assert!(
            restored
                .observe(actor)
                .unwrap()
                .ground_items
                .iter()
                .find(|i| i.id == ItemId(101))
                .unwrap()
                .identified
        );
    }
}
