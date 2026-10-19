//! Frozen draw allocation. Expertise changes may alter current demand, but a
//! paid draw never acquires permission to displace another item in its pool.
use crate::abilities::CapturedWeaponProfile;
use crate::content_ids::{EquipmentCapacityId, SpeciesId};
use crate::equipment_capacity::EquipmentCapacityUse;
use crate::{ActorId, AnatomySpec, EquipmentSlotId, Game, GameError, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EquipAnatomySource {
    Species { species: SpeciesId },
    Configured { anatomy: AnatomySpec },
}
impl<'de> Deserialize<'de> for EquipAnatomySource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Species { species: SpeciesId },
            Configured { anatomy: AnatomySpec },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Species { species } => Self::Species { species },
            Fields::Configured { anatomy } => Self::Configured { anatomy },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CapturedEquipOccupant {
    pub item: ItemId,
    pub socket: EquipmentSlotId,
    pub profile: CapturedWeaponProfile,
}
impl<'de> Deserialize<'de> for CapturedEquipOccupant {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            socket: EquipmentSlotId,
            profile: CapturedWeaponProfile,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            item: f.item,
            socket: f.socket,
            profile: f.profile,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FrozenEquipAllocation {
    pub anatomy: EquipAnatomySource,
    pub target: CapturedWeaponProfile,
    pub occupants: Vec<CapturedEquipOccupant>,
}
impl<'de> Deserialize<'de> for FrozenEquipAllocation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            anatomy: EquipAnatomySource,
            target: CapturedWeaponProfile,
            occupants: crate::bounded::Bounded<
                CapturedEquipOccupant,
                { crate::limits::EQUIPMENT_SOCKETS },
            >,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            anatomy: f.anatomy,
            target: f.target,
            occupants: f.occupants.0,
        })
    }
}
fn profile_demands(
    profile: &CapturedWeaponProfile,
    rules: &crate::rules::Ruleset,
) -> Option<(u16, u16, bool)> {
    match profile {
        CapturedWeaponProfile::Ordinary => Some((1, 1, false)),
        CapturedWeaponProfile::Profile { spec, expert } => {
            let validated = spec.validate(rules).ok()?;
            let state = validated.state(*expert);
            Some((
                u16::from(state.grip_demand()),
                u16::from(validated.maximum_grip_demand()),
                state.quickdraw(),
            ))
        }
    }
}
pub(crate) fn captured_profile_matches_surviving_item(
    game: &Game,
    item: ItemId,
    slot: &crate::content_ids::EquipmentSlot,
    profile: &CapturedWeaponProfile,
) -> bool {
    game.items.get(&item).is_none_or(|item| {
        item.spec.equipment.as_ref().is_some_and(|gear| {
            &gear.slot == slot
                && match (profile, &gear.weapon_profile) {
                    (CapturedWeaponProfile::Ordinary, None) => true,
                    (CapturedWeaponProfile::Profile { spec, .. }, Some(authored)) => {
                        spec == authored
                    }
                    _ => false,
                }
        })
    })
}
impl FrozenEquipAllocation {
    pub(crate) fn quickdraw(&self, rules: &crate::rules::Ruleset) -> bool {
        profile_demands(&self.target, rules).is_some_and(|(_, _, quickdraw)| quickdraw)
    }
    pub(crate) fn valid_shape(
        &self,
        rules: &crate::rules::Ruleset,
        target: ItemId,
        socket: EquipmentSlotId,
    ) -> bool {
        let mut items = std::collections::BTreeSet::new();
        target.0 > 0
            && usize::from(socket.0) < crate::limits::EQUIPMENT_SOCKETS
            && self.occupants.len() < crate::limits::EQUIPMENT_SOCKETS
            && profile_demands(&self.target, rules).is_some()
            && self
                .occupants
                .windows(2)
                .all(|rows| rows[0].socket < rows[1].socket)
            && self.occupants.iter().all(|row| {
                row.item.0 > 0
                    && row.item != target
                    && row.socket != socket
                    && usize::from(row.socket.0) < crate::limits::EQUIPMENT_SOCKETS
                    && items.insert(row.item)
                    && profile_demands(&row.profile, rules).is_some()
            })
            && match &self.anatomy {
                EquipAnatomySource::Species { .. } => true,
                EquipAnatomySource::Configured { anatomy } => {
                    anatomy.valid(rules)
                        && !matches!(
                            self.target,
                            CapturedWeaponProfile::Profile { expert: true, .. }
                        )
                        && self.occupants.iter().all(|row| {
                            !matches!(
                                row.profile,
                                CapturedWeaponProfile::Profile { expert: true, .. }
                            )
                        })
                }
            }
    }
    pub(crate) fn anatomy<'a>(&'a self, game: &'a Game, actor: ActorId) -> Option<&'a AnatomySpec> {
        match &self.anatomy {
            EquipAnatomySource::Species { species } => {
                Some(&game.content.catalog().species.get(species)?.anatomy)
            }
            EquipAnatomySource::Configured { anatomy } => {
                let owner = game.actors.get(&actor)?;
                (owner.combat.is_none() && *owner.anatomy == *anatomy).then_some(anatomy)
            }
        }
    }
    /// Structural source consistency, not a proof of historical expertise.
    pub(crate) fn source_valid(
        &self,
        game: &Game,
        actor: ActorId,
        item: ItemId,
        socket: EquipmentSlotId,
    ) -> bool {
        if !self.valid_shape(game.content.rules(), item, socket) || item.0 >= game.next_item_id {
            return false;
        }
        let Some(anatomy) = self.anatomy(game, actor) else {
            return false;
        };
        let Some(slot) = anatomy
            .slots
            .get(usize::from(socket.0))
            .and_then(|id| game.content.rules().equipment_slot(id))
        else {
            return false;
        };
        let Some((target_demand, maximum, _)) = profile_demands(&self.target, game.content.rules())
        else {
            return false;
        };
        if !captured_profile_matches_surviving_item(game, item, &slot.id, &self.target) {
            return false;
        }
        if !matches!(self.target, CapturedWeaponProfile::Ordinary) && !slot.requires_attack {
            return false;
        }
        match &slot.capacity {
            EquipmentCapacityUse::None => maximum == 1 && self.occupants.is_empty(),
            EquipmentCapacityUse::Shared { resource, units } => {
                let Some(capacity) = anatomy.capacities.get(resource) else {
                    return false;
                };
                let Some(mut total) = units.checked_mul(target_demand) else {
                    return false;
                };
                for row in &self.occupants {
                    if row.item.0 >= game.next_item_id {
                        return false;
                    }
                    let Some(other) = anatomy
                        .slots
                        .get(usize::from(row.socket.0))
                        .and_then(|id| game.content.rules().equipment_slot(id))
                    else {
                        return false;
                    };
                    if !captured_profile_matches_surviving_item(
                        game,
                        row.item,
                        &other.id,
                        &row.profile,
                    ) {
                        return false;
                    }
                    if !matches!(row.profile, CapturedWeaponProfile::Ordinary)
                        && !other.requires_attack
                    {
                        return false;
                    }
                    let EquipmentCapacityUse::Shared {
                        resource: other_resource,
                        units,
                    } = &other.capacity
                    else {
                        return false;
                    };
                    if other_resource != resource {
                        return false;
                    }
                    let Some((demand, _, _)) = profile_demands(&row.profile, game.content.rules())
                    else {
                        return false;
                    };
                    let Some(next) = units
                        .checked_mul(demand)
                        .and_then(|demand| total.checked_add(demand))
                    else {
                        return false;
                    };
                    total = next;
                }
                total <= capacity
            }
        }
    }
    pub(crate) fn resource<'a>(
        &'a self,
        game: &'a Game,
        actor: ActorId,
        socket: EquipmentSlotId,
    ) -> Option<&'a EquipmentCapacityId> {
        game.content
            .rules()
            .equipment_slot(
                self.anatomy(game, actor)?
                    .slots
                    .get(usize::from(socket.0))?,
            )?
            .capacity
            .resource()
    }
}
impl Game {
    pub(crate) fn capture_equip_allocation(
        &self,
        actor: ActorId,
        item: ItemId,
        socket: EquipmentSlotId,
    ) -> Result<FrozenEquipAllocation, GameError> {
        if !self.item_work_available(actor, &crate::Work::Equip { item, slot: socket }) {
            return Err(GameError::ItemUnavailable);
        }
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let profile = |item: ItemId| -> Result<CapturedWeaponProfile, GameError> {
            let item = self.items.get(&item).ok_or(GameError::ItemUnavailable)?;
            if item.location != ItemLocation::Carried(actor) {
                return Err(GameError::ItemUnavailable);
            }
            let gear = item
                .spec
                .equipment
                .as_ref()
                .ok_or(GameError::ItemUnavailable)?;
            Ok(gear
                .weapon_profile
                .as_ref()
                .map_or(CapturedWeaponProfile::Ordinary, |spec| {
                    CapturedWeaponProfile::Profile {
                        spec: spec.clone(),
                        expert: owner.combat.as_ref().is_some_and(|combat| {
                            combat
                                .creature()
                                .derived()
                                .expertises
                                .active(&spec.expertise)
                        }),
                    }
                }))
        };
        let slot = self
            .content
            .rules()
            .equipment_slot(
                owner
                    .anatomy
                    .slots
                    .get(usize::from(socket.0))
                    .ok_or(GameError::ItemUnavailable)?,
            )
            .ok_or(GameError::ItemUnavailable)?;
        let mut occupants = Vec::new();
        if let Some(resource) = slot.capacity.resource() {
            for (socket, item) in &owner.equipment {
                let other = self
                    .content
                    .rules()
                    .equipment_slot(&owner.anatomy.slots[usize::from(socket.0)])
                    .ok_or(GameError::ItemUnavailable)?;
                if other.capacity.resource() == Some(resource) {
                    occupants.push(CapturedEquipOccupant {
                        item: *item,
                        socket: *socket,
                        profile: profile(*item)?,
                    });
                }
            }
        }
        let captured = FrozenEquipAllocation {
            anatomy: owner.combat.as_ref().map_or_else(
                || EquipAnatomySource::Configured {
                    anatomy: (*owner.anatomy).clone(),
                },
                |combat| EquipAnatomySource::Species {
                    species: combat.creature().build().species_id().clone(),
                },
            ),
            target: profile(item)?,
            occupants,
        };
        captured
            .source_valid(self, actor, item, socket)
            .then_some(captured)
            .ok_or(GameError::ItemUnavailable)
    }
    pub(crate) fn frozen_equip_available(
        &self,
        actor: ActorId,
        item: ItemId,
        socket: EquipmentSlotId,
        captured: &FrozenEquipAllocation,
    ) -> bool {
        if !captured.source_valid(self, actor, item, socket)
            || !self.item_work_available(actor, &crate::Work::Equip { item, slot: socket })
        {
            return false;
        }
        let Some(owner) = self.actors.get(&actor) else {
            return false;
        };
        let Some(anatomy) = captured.anatomy(self, actor) else {
            return false;
        };
        let Some(slot) = self
            .content
            .rules()
            .equipment_slot(&anatomy.slots[usize::from(socket.0)])
        else {
            return false;
        };
        // Same relevant socket definitions and capacity, while unrelated pools may change.
        if owner.anatomy.slots.get(usize::from(socket.0))
            != anatomy.slots.get(usize::from(socket.0))
        {
            return false;
        }
        let Some(resource) = slot.capacity.resource() else {
            return captured.occupants.is_empty();
        };
        if owner.anatomy.capacities.get(resource) != anatomy.capacities.get(resource) {
            return false;
        }
        let current: Vec<_> = owner
            .equipment
            .iter()
            .filter(|(socket, _)| {
                owner
                    .anatomy
                    .slots
                    .get(usize::from(socket.0))
                    .and_then(|id| self.content.rules().equipment_slot(id))
                    .is_some_and(|slot| slot.capacity.resource() == Some(resource))
            })
            .map(|(socket, item)| (*socket, *item))
            .collect();
        current
            == captured
                .occupants
                .iter()
                .map(|row| (row.socket, row.item))
                .collect::<Vec<_>>()
            && captured.occupants.iter().all(|row| {
                owner.anatomy.slots.get(usize::from(row.socket.0))
                    == anatomy.slots.get(usize::from(row.socket.0))
                    && self
                        .items
                        .get(&row.item)
                        .is_some_and(|item| item.location == ItemLocation::Carried(actor))
            })
    }
}
