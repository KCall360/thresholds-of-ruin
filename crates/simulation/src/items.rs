use crate::{ActorId, Game, GameError, Item, ItemId, ItemLocation, OutcomeKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_world::Location;

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
pub(crate) struct PreparedTransfer {
    source: ItemId,
    expected_source: Item,
    result: ItemId,
    expected_destination: Option<Item>,
    destination: ItemLocation,
    quantity: u64,
    expected_next_item_id: u64,
}

impl PreparedTransfer {
    pub(crate) fn result(&self) -> ItemId {
        self.result
    }
}

/// Authoritative identity and physical stack properties. Never a client DTO.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
    pub magical: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub currency: Option<CurrencyKind>,
    pub equipment: Option<crate::EquipmentSpec>,
    pub consumable: Option<crate::ConsumableSpec>,
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
            currency: Option<CurrencyKind>,
            equipment: Option<crate::EquipmentSpec>,
            consumable: Option<crate::ConsumableSpec>,
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
            currency: fields.currency,
            equipment: fields.equipment,
            consumable: fields.consumable,
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
            currency: self.currency,
            equipment: self.equipment.clone(),
            consumable: self.consumable.clone(),
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
    pub fn ordinary(name: String) -> Self {
        Self {
            magical: false,
            currency: None,
            equipment: None,
            consumable: None,
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
        if let Some(consumable) = &self.consumable {
            rules.validate_effects(&consumable.effects)?;
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
        self.currency.is_none_or(|_| {
            self.class == ItemClass::Coin
                && self.stackable
                && self.equipment.is_none()
                && self.consumable.is_none()
                && self.edible.is_none()
                && self.supply.is_none()
        }) && self.equipment.as_ref().is_none_or(|equipment| {
            !self.stackable
                && self.consumable.is_none()
                && self.edible.is_none()
                && self.supply.is_none()
                && equipment.valid_shape()
        }) && self.consumable.as_ref().is_none_or(|consumable| {
            self.class == ItemClass::Potion && self.edible.is_none() && consumable.valid()
        }) && self.edible.as_ref().is_none_or(|edible| {
            edible.valid() && self.equipment.is_none() && self.consumable.is_none()
        }) && [
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
                            game.items[id].spec == item.spec
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
        let result = if let Some(id) = merge {
            let existing = self
                .items
                .get(&id)
                .filter(|existing| {
                    id != source
                        && item.spec.stackable
                        && existing.location == destination
                        && existing.spec == item.spec
                })
                .ok_or(GameError::ItemUnavailable)?;
            existing
                .quantity
                .checked_add(quantity)
                .ok_or(GameError::InvalidQuantity)?;
            id
        } else if quantity == item.quantity {
            source
        } else {
            self.next_item_id
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?;
            ItemId(self.next_item_id)
        };
        Ok(PreparedTransfer {
            source,
            expected_source: item.clone(),
            result,
            expected_destination: merge.map(|id| self.items[&id].clone()),
            destination,
            quantity,
            expected_next_item_id: self.next_item_id,
        })
    }

    pub(crate) fn apply_owned_transfer(&mut self, plan: PreparedTransfer) -> Result<(), GameError> {
        if self.next_item_id != plan.expected_next_item_id
            || self.items.get(&plan.source) != Some(&plan.expected_source)
            || (plan.source != plan.result
                && self.items.get(&plan.result) != plan.expected_destination.as_ref())
        {
            return Err(GameError::ItemUnavailable);
        }
        let ItemLocation::Carried(owner) = plan.expected_source.location else {
            return Err(GameError::ItemUnavailable);
        };
        if self
            .actors
            .get(&owner)
            .is_none_or(|actor| actor.equipment.values().any(|id| *id == plan.source))
        {
            return Err(GameError::ItemUnavailable);
        }
        self.apply_transfer(plan.source, plan.result, plan.quantity, plan.destination)
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
        let item = self.place_authored_item(id, at, spec.name.clone(), owner)?;
        self.items
            .edit(item, |entry| {
                entry.spec = tor_world::Shared::new(spec);
                entry.quantity = quantity;
            })
            .expect("placed item");
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
        let identity = self
            .items
            .get(&item)
            .ok_or(GameError::ItemUnavailable)?
            .spec
            .identity
            .clone();
        self.learn_identity(actor, &identity)
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
    ) -> Result<OutcomeKind, GameError> {
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
        let destination = if source.spec.stackable {
            self.items
                .at(to)
                .map(|id| (id, &self.items[&id]))
                .find(|(_, i)| {
                    crate::diagnostics::stack_candidate();
                    i.spec == source.spec
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
        let result = if let Some((id, existing)) = destination {
            existing
                .quantity
                .checked_add(quantity)
                .ok_or(GameError::InvalidQuantity)?;
            id
        } else if quantity == source.quantity {
            item
        } else {
            self.next_item_id
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?;
            ItemId(self.next_item_id)
        };
        Ok(if taking {
            OutcomeKind::Taken {
                item,
                result,
                quantity,
            }
        } else {
            OutcomeKind::Dropped {
                item,
                result,
                quantity,
            }
        })
    }

    pub(crate) fn apply_transfer(
        &mut self,
        source: ItemId,
        result: ItemId,
        quantity: u64,
        location: ItemLocation,
    ) -> Result<(), GameError> {
        let item = self.items.get(&source).ok_or(GameError::ItemUnavailable)?;
        if quantity == 0
            || quantity > item.quantity
            || (source == result && quantity != item.quantity)
        {
            return Err(GameError::InvalidQuantity);
        }
        if item.free_quantity(None).is_none_or(|free| quantity > free) {
            return Err(GameError::ItemUnavailable);
        }
        let destination_quantity = if source == result {
            None
        } else {
            self.items
                .get(&result)
                .map(|destination| {
                    destination
                        .quantity
                        .checked_add(quantity)
                        .ok_or(GameError::InvalidQuantity)
                })
                .transpose()?
        };
        let next_item_id = if source != result && destination_quantity.is_none() {
            if result != ItemId(self.next_item_id) {
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
                })
                .ok_or(GameError::ItemUnavailable)?;
            return Ok(());
        }
        let condition = self
            .items
            .get(&source)
            .ok_or(GameError::ItemUnavailable)?
            .condition;
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
        if let Some(total) = destination_quantity {
            self.items
                .edit(result, |existing| existing.quantity = total)
                .ok_or(GameError::ItemUnavailable)?;
        } else {
            self.items.insert(
                result,
                Item {
                    condition,
                    corpse: None,
                    holds: vec![],
                    motion,
                    orientation,
                    spec,
                    quantity,
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
                        armor: None,
                        slot: crate::content_ids::EquipmentSlot::new("shield").unwrap(),
                        attack: None,
                        modifiers: vec![],
                    })
                }
                6 => entry.spec.consumable = Some(crate::ConsumableSpec { effects: vec![] }),
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
        value["equipment"] = serde_json::json!({"slot":"shield", "attack":null, "modifiers":[]});
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
