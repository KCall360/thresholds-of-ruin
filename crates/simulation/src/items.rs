use crate::{ActorId, Game, GameError, Item, ItemId, ItemLocation, OutcomeKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_world::Location;

/// Authoritative identity and physical stack properties. Never a client DTO.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemSpec {
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

impl ItemSpec {
    pub fn ordinary(name: String) -> Self {
        Self {
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
    pub(crate) fn valid(&self) -> bool {
        [
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
        let item = self.place_authored_item(id, at, spec.name.clone(), owner)?;
        self.items
            .edit(item, |entry| {
                entry.spec = spec;
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

    pub(crate) fn prepare_transfer(
        &self,
        actor: ActorId,
        item: ItemId,
        requested: Option<u64>,
        taking: bool,
    ) -> Result<OutcomeKind, GameError> {
        let at = self.actors[&actor].location;
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
        let destination = if source.spec.stackable {
            self.items
                .at(to)
                .map(|id| (id, &self.items[&id]))
                .find(|(_, i)| {
                    crate::diagnostics::stack_candidate();
                    i.spec == source.spec
                        && (taking
                            || (i.motion == self.actors[&actor].motion
                                && i.orientation == self.actors[&actor].orientation)
                            || (i.motion == crate::MotionState::default()
                                && self.actors[&actor].motion == crate::MotionState::default()))
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
    ) {
        let (motion, orientation) = match (self.items[&source].location, location) {
            (ItemLocation::Carried(owner), ItemLocation::Ground(_)) => (
                self.actors[&owner].motion.clone(),
                self.actors[&owner].orientation,
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
                .expect("validated source");
            return;
        }
        let spec = self.items[&source].spec.clone();
        let remainder = self.items[&source].quantity - quantity;
        if remainder == 0 {
            self.items.remove(&source);
        } else {
            self.items
                .edit(source, |item| item.quantity = remainder)
                .expect("validated source");
        }
        if self
            .items
            .edit(result, |existing| existing.quantity += quantity)
            .is_none()
        {
            self.items.insert(
                result,
                Item {
                    motion,
                    orientation,
                    spec,
                    quantity,
                    location,
                },
            );
            self.next_item_id += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Position, RegionId};

    #[test]
    fn knowledge_survives_removal_of_last_instance_without_item_use_mechanics() {
        let mut game = Game::two_room(0);
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
        let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
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
