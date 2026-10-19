//! Exact reciprocal holding relationships and target-active re-grab protection.
//! This state is independent of perception, species and the effect that created it.
use crate::content_ids::FlagId;
use crate::{Actor, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Location;

pub const HOLD_CAPACITY: usize = 128;
/// Escalation shares the damage engine's checked numeric domain.
pub(crate) const HOLD_ESCALATION_MAX: u32 = crate::limits::NUMERIC as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HoldId(pub u64);
impl Serialize for HoldId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.0)
    }
}
impl<'de> Deserialize<'de> for HoldId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = u64::deserialize(deserializer)?;
        if value == 0 || value == u64::MAX {
            return Err(serde::de::Error::custom("invalid hold identity"));
        }
        Ok(Self(value))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct HoldPolicy {
    pub sustaining: FlagId,
    pub preparation: u64,
    pub recovery: u64,
    pub protection: u64,
}
impl HoldPolicy {
    pub fn valid(&self) -> bool {
        [self.preparation, self.recovery, self.protection]
            .into_iter()
            .all(|duration| duration > 0 && duration <= crate::limits::NUMERIC as u64)
    }
}
impl<'de> Deserialize<'de> for HoldPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            sustaining: FlagId,
            preparation: u64,
            recovery: u64,
            protection: u64,
        }
        let f: Fields = crate::bounded::named_map(deserializer)?;
        let value = Self {
            sustaining: f.sustaining,
            preparation: f.preparation,
            recovery: f.recovery,
            protection: f.protection,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid hold policy"));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LinkedHold {
    pub(crate) id: HoldId,
    pub(crate) holder: ActorId,
    pub(crate) policy: HoldPolicy,
    pub(crate) escalation: u32,
}
impl<'de> Deserialize<'de> for LinkedHold {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: HoldId,
            holder: ActorId,
            policy: HoldPolicy,
            escalation: u32,
        }
        let f: Fields = crate::bounded::named_map(deserializer)?;
        if f.holder.0 == 0 || f.holder.0 == u64::MAX {
            return Err(serde::de::Error::custom("invalid hold endpoint"));
        }
        if f.escalation > HOLD_ESCALATION_MAX {
            return Err(serde::de::Error::custom("invalid hold escalation"));
        }
        Ok(Self {
            id: f.id,
            holder: f.holder,
            policy: f.policy,
            escalation: f.escalation,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OutgoingHold {
    pub(crate) id: HoldId,
    pub(crate) target: ActorId,
}
impl<'de> Deserialize<'de> for OutgoingHold {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: HoldId,
            target: ActorId,
        }
        let f: Fields = crate::bounded::named_map(deserializer)?;
        if f.target.0 == 0 || f.target.0 == u64::MAX {
            return Err(serde::de::Error::custom("invalid hold endpoint"));
        }
        Ok(Self {
            id: f.id,
            target: f.target,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum HoldSlot {
    Linked { link: LinkedHold },
    Protection { holder: ActorId, remaining: u64 },
}
impl<'de> Deserialize<'de> for HoldSlot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Linked { link: LinkedHold },
            Protection { holder: ActorId, remaining: u64 },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Linked { link } => Self::Linked { link },
            Fields::Protection { holder, remaining } => {
                if holder.0 == 0
                    || holder.0 == u64::MAX
                    || remaining == 0
                    || remaining > crate::limits::NUMERIC as u64
                {
                    return Err(serde::de::Error::custom("invalid hold protection"));
                }
                Self::Protection { holder, remaining }
            }
        })
    }
}
impl HoldSlot {
    pub(crate) fn holder(&self) -> ActorId {
        match self {
            Self::Linked { link } => link.holder,
            Self::Protection { holder, .. } => *holder,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub(crate) struct HoldBook {
    pub(crate) incoming: Vec<HoldSlot>,
    pub(crate) outgoing: Vec<OutgoingHold>,
}
impl<'de> Deserialize<'de> for HoldBook {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            incoming: crate::bounded::Bounded<HoldSlot, HOLD_CAPACITY>,
            outgoing: crate::bounded::Bounded<OutgoingHold, HOLD_CAPACITY>,
        }
        let f: Fields = crate::bounded::named_map(deserializer)?;
        let value = Self {
            incoming: f.incoming.0,
            outgoing: f.outgoing.0,
        };
        if !value.valid_shape() {
            return Err(serde::de::Error::custom("invalid hold book"));
        }
        Ok(value)
    }
}
impl HoldBook {
    pub(crate) fn links(&self) -> impl Iterator<Item = &LinkedHold> {
        self.incoming.iter().filter_map(|slot| match slot {
            HoldSlot::Linked { link } => Some(link),
            _ => None,
        })
    }
    pub(crate) fn has_links(&self) -> bool {
        self.links().next().is_some() || !self.outgoing.is_empty()
    }
    pub(crate) fn peers(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.links()
            .map(|link| link.holder)
            .chain(self.outgoing.iter().map(|link| link.target))
    }
    pub(crate) fn horizon(&self) -> Option<u64> {
        self.incoming
            .iter()
            .filter_map(|slot| match slot {
                HoldSlot::Protection { remaining, .. } => Some(*remaining),
                _ => None,
            })
            .max()
    }
    pub(crate) fn next_change_in(&self) -> Option<u64> {
        self.incoming
            .iter()
            .filter_map(|slot| match slot {
                HoldSlot::Protection { remaining, .. } => Some(*remaining),
                _ => None,
            })
            .min()
    }
    pub(crate) fn advance(&mut self, elapsed: u64) {
        self.incoming.retain_mut(|slot| match slot {
            HoldSlot::Protection { remaining, .. } => {
                *remaining = remaining.saturating_sub(elapsed);
                *remaining != 0
            }
            HoldSlot::Linked { .. } => true,
        });
    }
    pub(crate) fn valid_shape(&self) -> bool {
        let mut holders = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        self.incoming.len() <= HOLD_CAPACITY
            && self.outgoing.len() <= HOLD_CAPACITY
            && self.incoming.iter().all(|slot| {
                slot.holder().0 > 0
                    && holders.insert(slot.holder())
                    && match slot {
                        HoldSlot::Linked { link } => {
                            link.id.0 > 0
                                && link.id.0 < u64::MAX
                                && ids.insert(link.id)
                                && link.policy.valid()
                                && link.escalation <= HOLD_ESCALATION_MAX
                        }
                        HoldSlot::Protection { remaining, .. } => {
                            *remaining > 0 && *remaining <= crate::limits::NUMERIC as u64
                        }
                    }
            })
            && self.outgoing.iter().all(|link| {
                link.id.0 > 0
                    && link.id.0 < u64::MAX
                    && link.target.0 > 0
                    && ids.insert(link.id)
                    && targets.insert(link.target)
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkedHoldObservation {
    pub id: HoldId,
    pub holder: ActorId,
}

fn invalid() -> GameError {
    GameError::Rule(crate::abilities::RuleError::InvalidDefinition)
}

impl Game {
    pub fn linked_holds(&self, actor: ActorId) -> Vec<LinkedHoldObservation> {
        self.actors
            .get(&actor)
            .into_iter()
            .flat_map(|actor| actor.holds.links())
            .map(|link| LinkedHoldObservation {
                id: link.id,
                holder: link.holder,
            })
            .collect()
    }
    pub(crate) fn linked_hold(&self, actor: ActorId, hold: HoldId) -> Option<&LinkedHold> {
        self.actors
            .get(&actor)?
            .holds
            .links()
            .find(|link| link.id == hold)
    }
    pub(crate) fn hold_contact(&self, holder: ActorId, target: ActorId) -> bool {
        let Some((holder, target)) = self.actors.get(&holder).zip(self.actors.get(&target)) else {
            return false;
        };
        let Some(source) = self.body_cells(holder.location, holder.orientation, &holder.body)
        else {
            return false;
        };
        let Some(destination) = self.body_cells(target.location, target.orientation, &target.body)
        else {
            return false;
        };
        let cells: BTreeSet<_> = destination.iter().map(|(at, _)| *at).collect();
        source.iter().any(|(from, _)| {
            (-1..=1).any(|x| {
                (-1..=1).any(|y| {
                    (-1..=1).any(|z| {
                        [x, y, z] != [0, 0, 0]
                            && self
                                .melee_neighbor_frame(*from, [x, y, z])
                                .is_some_and(|(at, _)| cells.contains(&at))
                    })
                })
            })
        })
    }
    fn hold_endpoints_valid(&self, holder: ActorId, target: ActorId, policy: &HoldPolicy) -> bool {
        holder != target
            && self.actors.get(&target).is_some_and(Actor::alive)
            && self.actors.get(&holder).is_some_and(|actor| {
                actor.alive()
                    && actor.combat.as_ref().is_some_and(|combat| {
                        combat
                            .creature()
                            .derived()
                            .flags
                            .contains(&policy.sustaining)
                    })
            })
            && self.hold_contact(holder, target)
    }
    pub(crate) fn valid_linked_hold(&self, holder: ActorId, target: ActorId, hold: HoldId) -> bool {
        self.linked_hold(target, hold).is_some_and(|link| {
            link.holder == holder
                && self.hold_endpoints_valid(holder, target, &link.policy)
                && self.actors[&holder]
                    .holds
                    .outgoing
                    .contains(&OutgoingHold { id: hold, target })
        })
    }
    pub fn apply_linked_hold(
        &mut self,
        holder: ActorId,
        target: ActorId,
        policy: HoldPolicy,
    ) -> Result<Option<HoldId>, GameError> {
        self.apply_linked_hold_with_publication(holder, target, policy, true)
    }
    /// Rider participation remains unpublished until its outer strike finishes.
    pub(crate) fn apply_linked_hold_in_transaction(
        &mut self,
        holder: ActorId,
        target: ActorId,
        policy: HoldPolicy,
    ) -> Result<Option<HoldId>, GameError> {
        self.apply_linked_hold_with_publication(holder, target, policy, false)
    }
    fn apply_linked_hold_with_publication(
        &mut self,
        holder: ActorId,
        target: ActorId,
        policy: HoldPolicy,
        publish: bool,
    ) -> Result<Option<HoldId>, GameError> {
        if !policy.valid() {
            return Err(invalid());
        }
        // Duplicate and protection checks precede capacity and identity allocation.
        let target_state = self.actors.get(&target).ok_or(GameError::UnknownActor)?;
        if let Some(link) = target_state
            .holds
            .links()
            .find(|link| link.holder == holder)
        {
            return Ok(Some(link.id));
        }
        let elapsed = self
            .actor_clock(target)
            .saturating_sub(target_state.active_since);
        if target_state.holds.incoming.iter().any(|slot| matches!(slot, HoldSlot::Protection { holder: owner, remaining } if *owner == holder && *remaining > elapsed)) { return Ok(None); }
        let mut candidate = self.clone();
        if publish {
            candidate.begin_effect_publication()?;
        }
        candidate.settle_actor_time(holder)?;
        candidate.settle_actor_time(target)?;
        if !candidate.hold_endpoints_valid(holder, target, &policy) {
            return Ok(None);
        }
        if candidate.actors[&target].holds.incoming.len() >= HOLD_CAPACITY
            || candidate.actors[&holder].holds.outgoing.len() >= HOLD_CAPACITY
        {
            return Err(GameError::Rule(
                crate::abilities::RuleError::ConditionCapacity,
            ));
        }
        if candidate.next_hold_id == 0 || candidate.next_hold_id == u64::MAX {
            return Err(GameError::IdentityExhausted);
        }
        let id = HoldId(candidate.next_hold_id);
        candidate.next_hold_id += 1;
        candidate
            .actors
            .get_mut(&target)
            .expect("validated target")
            .holds
            .incoming
            .push(HoldSlot::Linked {
                link: LinkedHold {
                    id,
                    holder,
                    policy,
                    escalation: 0,
                },
            });
        candidate
            .actors
            .get_mut(&holder)
            .expect("validated holder")
            .holds
            .outgoing
            .push(OutgoingHold { id, target });
        if publish {
            candidate.finish_effect_publication()?;
        }
        *self = candidate;
        Ok(Some(id))
    }
    pub(crate) fn release_linked_hold(
        &mut self,
        target: ActorId,
        hold: HoldId,
    ) -> Result<bool, GameError> {
        let Some(holder) = self.linked_hold(target, hold).map(|link| link.holder) else {
            return Ok(false);
        };
        if !self.actors.get(&holder).is_some_and(|state| {
            state
                .holds
                .outgoing
                .contains(&OutgoingHold { id: hold, target })
        }) {
            return Err(invalid());
        }
        self.actors
            .get_mut(&target)
            .expect("live target")
            .holds
            .incoming
            .retain(|slot| !matches!(slot, HoldSlot::Linked { link } if link.id == hold));
        self.actors
            .get_mut(&holder)
            .expect("live holder")
            .holds
            .outgoing
            .retain(|link| link.id != hold);
        Ok(true)
    }
    pub(crate) fn escape_linked_hold(
        &mut self,
        target: ActorId,
        hold: HoldId,
    ) -> Result<bool, GameError> {
        let Some(link) = self.linked_hold(target, hold).cloned() else {
            return Ok(false);
        };
        if !self.valid_linked_hold(link.holder, target, hold) {
            return Ok(false);
        }
        self.actor_clock(target)
            .checked_add(link.policy.protection)
            .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        let mut candidate = self.clone();
        candidate.settle_actor_time(link.holder)?;
        candidate.settle_actor_time(target)?;
        if !candidate.valid_linked_hold(link.holder, target, hold) {
            return Ok(false);
        }
        let mut state = candidate.actors.get_mut(&target).expect("live target");
        let slot = state
            .holds
            .incoming
            .iter_mut()
            .find(|slot| matches!(slot, HoldSlot::Linked { link } if link.id == hold))
            .expect("exact link");
        *slot = HoldSlot::Protection {
            holder: link.holder,
            remaining: link.policy.protection,
        };
        drop(state);
        candidate
            .actors
            .get_mut(&link.holder)
            .expect("live holder")
            .holds
            .outgoing
            .retain(|link| link.id != hold);
        *self = candidate;
        Ok(true)
    }
    pub(crate) fn advance_linked_hold(
        &mut self,
        holder: ActorId,
        target: ActorId,
        hold: HoldId,
        increment: u32,
    ) -> Result<Option<u32>, GameError> {
        if !self.valid_linked_hold(holder, target, hold) {
            return Ok(None);
        }
        let next = self
            .linked_hold(target, hold)
            .expect("validated link")
            .escalation
            .checked_add(increment)
            .filter(|next| *next <= HOLD_ESCALATION_MAX)
            .ok_or(invalid())?;
        let mut state = self.actors.get_mut(&target).expect("live target");
        let link = state
            .holds
            .incoming
            .iter_mut()
            .find_map(|slot| match slot {
                HoldSlot::Linked { link } if link.id == hold => Some(link),
                _ => None,
            })
            .expect("exact link");
        link.escalation = next;
        Ok(Some(next))
    }
    pub(crate) fn hold_capability_lost(&self, id: ActorId, candidate: &Actor) -> bool {
        let Some(previous) = self.actors.get(&id) else {
            return false;
        };
        if !candidate.alive() && (previous.holds.has_links() || candidate.holds.has_links()) {
            return true;
        }
        previous
            .holds
            .outgoing
            .iter()
            .chain(&candidate.holds.outgoing)
            .any(|outgoing| {
                self.linked_hold(outgoing.target, outgoing.id)
                    .is_some_and(|link| {
                        candidate.combat.as_ref().is_none_or(|combat| {
                            !combat
                                .creature()
                                .derived()
                                .flags
                                .contains(&link.policy.sustaining)
                        })
                    })
            })
    }
    pub(crate) fn reconcile_holds_for_actor(&mut self, actor: ActorId) -> Result<(), GameError> {
        let Some(state) = self.actors.get(&actor) else {
            return Ok(());
        };
        let links: Vec<_> = state
            .holds
            .links()
            .map(|link| (link.holder, actor, link.id))
            .chain(
                state
                    .holds
                    .outgoing
                    .iter()
                    .map(|link| (actor, link.target, link.id)),
            )
            .collect();
        // Validate the whole local reciprocal set before any retirement. Corrupt
        // mirrors are rejected, never silently repaired by a lifecycle hook.
        if links.iter().any(|(holder, target, hold)| {
            !self
                .linked_hold(*target, *hold)
                .is_some_and(|link| link.holder == *holder)
                || !self.actors.get(holder).is_some_and(|state| {
                    state.holds.outgoing.contains(&OutgoingHold {
                        id: *hold,
                        target: *target,
                    })
                })
        }) {
            return Err(invalid());
        }
        for (holder, target, hold) in links {
            if !self.valid_linked_hold(holder, target, hold) {
                self.release_linked_hold(target, hold)?;
            }
        }
        Ok(())
    }
    pub(crate) fn reconcile_holds_for_actors(
        &mut self,
        actors: impl IntoIterator<Item = ActorId>,
    ) -> Result<(), GameError> {
        for actor in actors {
            self.reconcile_holds_for_actor(actor)?;
        }
        Ok(())
    }
    /// Capture before a geometry change. Body occupancy is sparse and the
    /// one-cell neighborhood includes every possible local contact witness.
    pub(crate) fn hold_geometry_candidates(
        &self,
        cells: impl IntoIterator<Item = Location>,
    ) -> BTreeSet<ActorId> {
        let mut owners = BTreeSet::new();
        for at in cells {
            owners.extend(self.actors.at(&self.world, at).keys().copied());
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        if let Some((neighbor, _)) = self.melee_neighbor_frame(at, [x, y, z]) {
                            owners.extend(self.actors.at(&self.world, neighbor).keys().copied());
                        }
                    }
                }
            }
        }
        owners.retain(|id| self.actors[id].holds.has_links());
        owners
    }
    pub(crate) fn hold_actor_shape_valid(&self, id: ActorId, actor: &Actor) -> bool {
        actor.holds.valid_shape()
            && actor
                .holds
                .horizon()
                .is_none_or(|horizon| actor.active_since.checked_add(horizon).is_some())
            && actor.holds.incoming.iter().all(|slot| {
                slot.holder() != id
                    && slot.holder().0 < self.next_actor_id
                    && match slot {
                        HoldSlot::Linked { link } => {
                            link.id.0 < self.next_hold_id
                                && actor.alive()
                                && self.known_actor_region(link.holder).is_some()
                        }
                        HoldSlot::Protection { .. } => true,
                    }
            })
            && actor.holds.outgoing.iter().all(|link| {
                link.target != id
                    && link.id.0 < self.next_hold_id
                    && link.target.0 < self.next_actor_id
                    && actor.alive()
                    && self.known_actor_region(link.target).is_some()
            })
    }
    pub(crate) fn loaded_holds_valid(&self) -> bool {
        let mut ids = BTreeSet::new();
        self.actors.iter().all(|(id, actor)| {
            self.hold_actor_shape_valid(*id, actor)
                && actor.holds.links().all(|link| {
                    ids.insert(link.id) && self.valid_linked_hold(link.holder, *id, link.id)
                })
                && actor
                    .holds
                    .outgoing
                    .iter()
                    .all(|link| self.valid_linked_hold(*id, link.target, link.id))
        })
    }
    /// Audit cross-record identity/mirror/capability state without requiring
    /// detached geometry. Contact is checked when its dependency closure arrives.
    pub(crate) fn all_hold_records_valid(
        &self,
        actors: &BTreeMap<ActorId, &Actor>,
        items: &BTreeMap<crate::ItemId, &crate::Item>,
    ) -> bool {
        let sources = crate::abilities::PlanSourceContext::Records { actors, items };
        let mut ids = BTreeSet::new();
        actors.iter().all(|(id, actor)| {
            self.hold_actor_shape_valid(*id, actor)
                && actor.pending.as_ref().is_none_or(|pending| {
                    pending.plan.actor() == *id
                        && self.plan_source_valid_with_context(&pending.plan, sources)
                        && pending
                            .progress
                            .hold_binding()
                            .is_none_or(|binding| binding.id.0 < self.next_hold_id)
                })
                && actor.holds.links().all(|link| {
                    ids.insert(link.id)
                        && actors.get(&link.holder).is_some_and(|holder| {
                            holder.alive()
                                && holder.combat.as_ref().is_some_and(|combat| {
                                    combat
                                        .creature()
                                        .derived()
                                        .flags
                                        .contains(&link.policy.sustaining)
                                })
                                && holder.holds.outgoing.contains(&OutgoingHold {
                                    id: link.id,
                                    target: *id,
                                })
                        })
                })
                && actor.holds.outgoing.iter().all(|link| {
                    actors.get(&link.target).is_some_and(|target| {
                        target
                            .holds
                            .links()
                            .any(|incoming| incoming.id == link.id && incoming.holder == *id)
                    })
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streaming::{MemoryRecords, RecordStore, RegionTransition};
    use std::num::NonZeroU64;
    use tor_world::{Extent, Position, Region, RegionId, Shared, World};

    fn policy() -> HoldPolicy {
        HoldPolicy {
            sustaining: FlagId::new("magical").unwrap(),
            preparation: 100,
            recovery: 100,
            protection: 400,
        }
    }
    fn build() -> crate::creatures::CreatureBuild {
        let mut species = crate::test_creatures::species();
        species.modifiers.push(crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Flag {
                flag: policy().sustaining,
            },
        });
        crate::test_creatures::build(species)
    }
    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }
    fn fixture(count: usize) -> Game {
        fixture_with_width(count, 1600)
    }
    fn fixture_with_width(count: usize, width: i32) -> Game {
        let build = build();
        let world = World::new(
            (1..=3)
                .map(|id| Region {
                    id: RegionId(id),
                    name: format!("held {id}"),
                    bounds: Extent::new(width, 4, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let mut game = Game::new(world, 42, build.content().clone());
        for region in 1..=3 {
            game.set_gravity(RegionId(region), [0; 3]).unwrap();
        }
        for x in 1..=count {
            let actor = game
                .spawn_actor(at(1, x as i32), NonZeroU64::new(100).unwrap())
                .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: format!("actor {x}"),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        game
    }
    fn wait_to(game: &mut Game, tick: u64) {
        while game.tick() < tick {
            let actor = game.next_actor().unwrap();
            game.act(actor, crate::Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), tick);
    }
    #[test]
    fn linked_hold_two_holders_exact_release_and_ordinary_movement() {
        let mut game = fixture(3);
        let first = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        let second = game
            .apply_linked_hold(ActorId(3), ActorId(2), policy())
            .unwrap()
            .unwrap();
        assert_eq!(game.linked_holds(ActorId(2)).len(), 2);
        assert!(game.condition_restricts(
            ActorId(2),
            crate::rules::ConditionRestriction::OrdinaryMovement
        ));
        assert!(!game.condition_restricts(
            ActorId(2),
            crate::rules::ConditionRestriction::MovementAbilities
        ));
        assert!(game.escape_linked_hold(ActorId(2), first).unwrap());
        assert_eq!(
            game.linked_holds(ActorId(2)),
            vec![LinkedHoldObservation {
                id: second,
                holder: ActorId(3)
            }]
        );
        assert!(game.condition_restricts(
            ActorId(2),
            crate::rules::ConditionRestriction::OrdinaryMovement
        ));
        game.release_linked_hold(ActorId(2), second).unwrap();
        assert!(!game.condition_restricts(
            ActorId(2),
            crate::rules::ConditionRestriction::OrdinaryMovement
        ));
        assert!(game.loaded_holds_valid());
    }
    #[test]
    fn linked_hold_protection_exact_active_boundary_and_no_allocation() {
        let mut game = fixture(2);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        let highwater = game.next_hold_id;
        assert_eq!(
            game.apply_linked_hold(ActorId(1), ActorId(2), policy())
                .unwrap(),
            Some(id)
        );
        assert_eq!(game.next_hold_id, highwater);
        game.escape_linked_hold(ActorId(2), id).unwrap();
        game.actors.get_mut(&ActorId(2)).unwrap().combat = None;
        assert!(game
            .actors
            .scheduled_timers()
            .any(|(deadline, actor)| deadline == 400 && actor == ActorId(2)));
        game.tick = 399;
        game.settle_actor_time(ActorId(2)).unwrap();
        assert_eq!(
            game.apply_linked_hold(ActorId(1), ActorId(2), policy())
                .unwrap(),
            None
        );
        assert_eq!(game.next_hold_id, highwater);
        game.tick = 400;
        game.settle_actor_time(ActorId(2)).unwrap();
        assert!(game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .is_some_and(|next| next != id));
    }
    #[test]
    fn linked_hold_full_target_converts_in_place_and_refuses_new_atomically() {
        let mut game = fixture(130);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        for holder in 4..=130 {
            game.actors
                .get_mut(&ActorId(2))
                .unwrap()
                .holds
                .incoming
                .push(HoldSlot::Protection {
                    holder: ActorId(holder),
                    remaining: 400,
                });
        }
        assert_eq!(game.actors[&ActorId(2)].holds.incoming.len(), HOLD_CAPACITY);
        let before = game.clone();
        assert_eq!(
            game.apply_linked_hold(ActorId(3), ActorId(2), policy()),
            Err(GameError::Rule(
                crate::abilities::RuleError::ConditionCapacity
            ))
        );
        assert_eq!(game, before);
        assert_eq!(
            game.apply_linked_hold(ActorId(1), ActorId(2), policy())
                .unwrap(),
            Some(id)
        );
        game.escape_linked_hold(ActorId(2), id).unwrap();
        assert_eq!(game.actors[&ActorId(2)].holds.incoming.len(), HOLD_CAPACITY);
        assert!(game.actors[&ActorId(1)].holds.outgoing.is_empty());
        assert_eq!(
            game.apply_linked_hold(ActorId(1), ActorId(2), policy())
                .unwrap(),
            None
        );
    }
    #[test]
    fn linked_hold_teleport_retirement_regrab_and_rewind_highwater() {
        let mut game = fixture(2);
        let past = game.clone();
        let old = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        game.teleport(ActorId(1), at(1, 10)).unwrap();
        assert!(game.linked_holds(ActorId(2)).is_empty());
        game.teleport(ActorId(1), at(1, 1)).unwrap();
        let new = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        assert!(new > old);
        assert_eq!(
            game.advance_linked_hold(ActorId(1), ActorId(2), old, 1)
                .unwrap(),
            None
        );
        let mut rewound = past;
        rewound.continue_identities(&game);
        assert!(
            rewound
                .apply_linked_hold(ActorId(1), ActorId(2), policy())
                .unwrap()
                .unwrap()
                > new
        );
    }
    #[test]
    fn linked_hold_escalation_is_exact_checked_and_initially_zero() {
        let mut game = fixture(2);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 0);
        assert_eq!(
            game.advance_linked_hold(ActorId(1), ActorId(2), id, 1)
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            game.advance_linked_hold(ActorId(1), ActorId(2), id, u32::MAX),
            Err(invalid())
        );
        assert_eq!(game.linked_hold(ActorId(2), id).unwrap().escalation, 1);
        assert_eq!(
            game.advance_linked_hold(ActorId(1), ActorId(2), id, HOLD_ESCALATION_MAX - 1)
                .unwrap(),
            Some(HOLD_ESCALATION_MAX)
        );
        let at_limit = game.clone();
        assert_eq!(
            game.advance_linked_hold(ActorId(1), ActorId(2), id, 1),
            Err(invalid())
        );
        assert_eq!(game, at_limit);
        let mut oversized = serde_json::to_value(&game.actors[&ActorId(2)].holds).unwrap();
        oversized["incoming"][0]["link"]["escalation"] = serde_json::json!(HOLD_ESCALATION_MAX + 1);
        assert!(serde_json::from_value::<HoldBook>(oversized).is_err());
        game.next_hold_id = u64::MAX;
        assert_eq!(
            game.apply_linked_hold(ActorId(1), ActorId(2), policy())
                .unwrap(),
            Some(id)
        );
        game.release_linked_hold(ActorId(2), id).unwrap();
        let before = game.clone();
        assert_eq!(
            game.apply_linked_hold(ActorId(1), ActorId(2), policy()),
            Err(GameError::IdentityExhausted)
        );
        assert_eq!(game, before);
    }
    #[test]
    fn linked_hold_mutation_rejects_broken_mirrors_atomically() {
        let mut game = fixture(2);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .holds
            .outgoing
            .clear();
        let before = game.clone();
        assert_eq!(game.release_linked_hold(ActorId(2), id), Err(invalid()));
        assert_eq!(game.reconcile_holds_for_actor(ActorId(2)), Err(invalid()));
        assert_eq!(game, before);
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .holds
            .outgoing
            .push(OutgoingHold {
                id,
                target: ActorId(2),
            });
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .holds
            .incoming
            .clear();
        let before = game.clone();
        assert_eq!(game.reconcile_holds_for_actor(ActorId(1)), Err(invalid()));
        assert_eq!(game, before);
    }
    #[test]
    fn linked_hold_death_retires_only_affected_mirrors() {
        let mut game = fixture(3);
        game.apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap();
        let retained = game
            .apply_linked_hold(ActorId(3), ActorId(2), policy())
            .unwrap()
            .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(1), 100_000).unwrap();
        assert_eq!(
            game.linked_holds(ActorId(2)),
            vec![LinkedHoldObservation {
                id: retained,
                holder: ActorId(3)
            }]
        );
        crate::test_creatures::injure(&mut game, ActorId(2), 100_000).unwrap();
        assert!(game.actors[&ActorId(3)].holds.outgoing.is_empty());
        assert!(game.loaded_holds_valid());
    }
    #[test]
    fn linked_hold_strict_current_records_and_highwater() {
        for value in [
            serde_json::json!(0),
            serde_json::json!(u64::MAX),
            serde_json::json!("1"),
            serde_json::json!({"value": 1}),
            serde_json::json!([1]),
        ] {
            assert!(serde_json::from_value::<HoldId>(value).is_err());
        }
        let mut game = fixture(2);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        let value = serde_json::to_value(&game.actors[&ActorId(2)].holds).unwrap();
        assert!(serde_json::from_value::<HoldBook>(value.clone()).is_ok());
        let mut duplicate = value.clone();
        duplicate["incoming"]
            .as_array_mut()
            .unwrap()
            .push(value["incoming"][0].clone());
        assert!(serde_json::from_value::<HoldBook>(duplicate).is_err());
        for field in ["incoming", "outgoing"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<HoldBook>(missing).is_err());
        }
        let mut positional = value.clone();
        positional["incoming"][0]["link"]["policy"] = serde_json::json!(["magical", 100, 100, 400]);
        assert!(serde_json::from_value::<HoldBook>(positional).is_err());
        let mut shared = crate::checkpoint::SharedState::default();
        let saved = game.checkpoint(&mut shared);
        let mut saved = serde_json::to_value(saved).unwrap();
        saved["next_hold_id"] = serde_json::json!(id.0);
        let saved = serde_json::from_value(saved).unwrap();
        assert!(Game::restore_checkpoint(saved, &shared, game.content().clone()).is_err());
    }
    #[test]
    fn linked_hold_component_exceeds_slot_budget_and_index_stays_sparse() {
        let mut game = fixture(130);
        for actor in 1..130 {
            game.apply_linked_hold(ActorId(actor), ActorId(actor + 1), policy())
                .unwrap()
                .unwrap();
        }
        let transition = RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::from([RegionId(1)]),
        };
        let (_, before) = game.settle_counted(&transition);
        assert_eq!(
            (
                before.hold_owners,
                before.hold_references,
                before.hold_regions
            ),
            (130, 258, 1)
        );
        for x in 500..600 {
            game.spawn_actor(at(1, x), NonZeroU64::new(100).unwrap())
                .unwrap();
        }
        let (_, after) = game.settle_counted(&transition);
        assert_eq!(
            (after.hold_owners, after.hold_references, after.hold_regions),
            (130, 258, 1)
        );
        let mut shared = crate::checkpoint::SharedState::default();
        let saved = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap(),
            game
        );
    }
    #[test]
    fn linked_hold_frozen_protection_resumes_remaining_without_peer_pin() {
        let mut game = fixture(2);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        game.escape_linked_hold(ActorId(2), id).unwrap();
        game.actors.get_mut(&ActorId(2)).unwrap().combat = None;
        wait_to(&mut game, 100);
        game.teleport(ActorId(1), at(2, 1)).unwrap();
        let runner = game
            .spawn_actor(at(3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut records = MemoryRecords::default();
        let frozen = RegionTransition {
            active: BTreeSet::from([RegionId(3)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
        };
        game.apply_region_transition(&frozen, &mut records).unwrap();
        assert_eq!(game.actors[&ActorId(2)].holds.horizon(), Some(300));
        assert!(
            game.detached_actor(ActorId(1)),
            "protection cannot retain its former holder"
        );
        while game.tick() < 10_000 {
            game.act(runner, crate::Action::Wait).unwrap();
        }
        let thawed = RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(3)]),
            loaded: frozen.loaded,
        };
        game.apply_region_transition(&thawed, &mut records).unwrap();
        assert_eq!(game.actors[&ActorId(2)].active_since, 10_000);
        assert_eq!(game.actors[&ActorId(2)].holds.horizon(), Some(300));
        assert!(game
            .actors
            .scheduled_timers()
            .any(|(deadline, actor)| deadline == 10_300 && actor == ActorId(2)));
        game.tick = 10_299;
        game.settle_actor_time(ActorId(2)).unwrap();
        assert_eq!(game.actors[&ActorId(2)].holds.horizon(), Some(1));
        game.tick = 10_300;
        game.settle_actor_time(ActorId(2)).unwrap();
        assert_eq!(game.actors[&ActorId(2)].holds.horizon(), None);
    }
    struct CorruptRecords {
        records: BTreeMap<crate::RecordId, Shared<crate::RegionRecord>>,
    }
    impl RecordStore for CorruptRecords {
        fn get(&mut self, id: crate::RecordId) -> Option<Shared<crate::RegionRecord>> {
            self.records.get(&id).cloned()
        }
        fn put(&mut self, id: crate::RecordId, record: Shared<crate::RegionRecord>) {
            self.records.insert(id, record);
        }
    }
    #[test]
    fn linked_hold_detached_mirror_audit_and_arrival_reject_atomically() {
        let mut game = fixture(2);
        game.apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        game.spawn_actor(at(3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut records = MemoryRecords::default();
        game.apply_region_transition(
            &RegionTransition {
                active: BTreeSet::from([RegionId(3)]),
                loaded: BTreeSet::from([RegionId(3)]),
            },
            &mut records,
        )
        .unwrap();
        assert!(game.detached_records_valid(&mut records));
        let mut damaged = CorruptRecords {
            records: BTreeMap::new(),
        };
        for (_, id) in game.detached_records() {
            let record = records.get(id).unwrap();
            let mut value =
                serde_json::to_value(crate::RegionRecordData::capture(&record)).unwrap();
            if value["actors"].get("1").is_some() {
                value["actors"]["1"]["holds"]["outgoing"] = serde_json::json!([]);
            }
            let record = serde_json::from_value::<crate::RegionRecordData>(value)
                .unwrap()
                .restore(game.content().clone())
                .unwrap();
            damaged.put(id, Shared::new(record));
        }
        assert!(!game.detached_records_valid(&mut damaged));
        let before = game.clone();
        assert!(game
            .transition_regions(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(1)]),
                    loaded: BTreeSet::from([RegionId(1), RegionId(3)])
                },
                &mut damaged
            )
            .is_err());
        assert_eq!(game, before);
    }
    #[test]
    fn linked_hold_swept_physics_retires_before_return_to_contact() {
        let mut game = fixture_with_width(3, 5);
        let at = |x, y| Location {
            region: RegionId(1),
            position: Position { x, y, z: 0 },
        };
        let (target, stationary, moving) = (ActorId(1), ActorId(2), ActorId(3));
        game.teleport(stationary, at(0, 1)).unwrap();
        game.teleport(moving, at(1, 2)).unwrap();
        game.connect_portal_area(
            tor_world::Passage {
                from: at(4, 2),
                direction: tor_world::Direction::East,
                to: at(0, 2),
            },
            0,
            1,
            1,
        )
        .unwrap();
        let preserved = game
            .apply_linked_hold(stationary, target, policy())
            .unwrap()
            .unwrap();
        let retired = game
            .apply_linked_hold(moving, target, policy())
            .unwrap()
            .unwrap();
        for actor in [target, stationary, moving] {
            game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(32).unwrap();
        }
        game.set_actor_velocity(moving, [8192, 0, 0]).unwrap();
        for actor in [target, stationary, moving] {
            game.act(actor, crate::Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 32);
        assert_eq!(game.actors[&moving].location, at(0, 2));
        assert!(game.hold_contact(moving, target));
        assert!(game.linked_hold(target, retired).is_none());
        assert!(game.actors[&moving].holds.outgoing.is_empty());
        assert!(game.linked_hold(target, preserved).is_some());
    }
    #[test]
    fn linked_hold_tall_door_upper_cell_close_and_insertion_retire_mirrors() {
        for insertion in [false, true] {
            let build = build();
            let world = World::new(
                vec![Region {
                    id: RegionId(1),
                    name: "tall contact".into(),
                    bounds: Extent::new(6, 6, 6).unwrap(),
                }],
                vec![],
            )
            .unwrap();
            let mut game = Game::new(world, 42, build.content().clone());
            game.set_gravity(RegionId(1), [0; 3]).unwrap();
            let at = |x, y, z| Location {
                region: RegionId(1),
                position: Position { x, y, z },
            };
            for location in [at(1, 1, 0), at(1, 1, 4), at(2, 2, 4)] {
                let actor = game
                    .spawn_actor(location, NonZeroU64::new(100).unwrap())
                    .unwrap();
                game.configure_creature(
                    actor,
                    crate::CreatureIdentity {
                        name: "door subject".into(),
                        faction: "neutral".into(),
                    },
                    build.clone(),
                )
                .unwrap();
            }
            let door = (!insertion).then(|| game.place_door(at(2, 1, 0), true, 5).unwrap());
            let id = game
                .apply_linked_hold(ActorId(2), ActorId(3), policy())
                .unwrap()
                .unwrap();
            if let Some(door) = door {
                game.act(ActorId(1), crate::Action::SetDoor { door, open: false })
                    .unwrap();
            } else {
                game.place_door(at(2, 1, 0), false, 5).unwrap();
            }
            assert!(!game.hold_contact(ActorId(2), ActorId(3)));
            assert!(game.linked_hold(ActorId(3), id).is_none());
            assert!(game.actors[&ActorId(2)].holds.outgoing.is_empty());
        }
    }
    #[test]
    fn linked_hold_supplemental_endpoints_preserve_strength_without_floor_feedback() {
        use crate::floor_activation::{
            PopulationPlan, PopulationProfile, PreparedScope, ScenarioCapabilities,
        };
        let mut game = fixture(2);
        game.teleport(ActorId(1), at(1, 1599)).unwrap();
        game.teleport(ActorId(2), at(2, 0)).unwrap();
        game.connect_portal_area(
            tor_world::Passage {
                from: at(1, 1599),
                direction: tor_world::Direction::East,
                to: at(2, 0),
            },
            0,
            1,
            1,
        )
        .unwrap();
        game.apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
            .unwrap();
        game.configure_population(PopulationPlan {
            protagonist: ActorId(1),
            carried_item_pools: BTreeMap::new(),
            profiles: BTreeMap::from([(
                "plain".into(),
                PopulationProfile {
                    version: 1,
                    stages: vec![],
                },
            )]),
        })
        .unwrap();
        game.register_activation_scope(PreparedScope {
            rooms: vec![],
            scope: "dungeon/2".into(),
            depth: 2,
            profile: "plain".into(),
            members: BTreeSet::from([RegionId(2), RegionId(3)]),
            cells: vec![at(2, 20), at(3, 20)],
            protected: BTreeSet::new(),
        })
        .unwrap();
        let frontier = RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::from([RegionId(1)]),
        };
        let plan = game.settle_region_transition(&frontier);
        assert!(plan.active.is_empty());
        assert_eq!(plan.loaded, BTreeSet::from([RegionId(1), RegionId(2)]));
        let active = game.settle_region_transition(&RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: frontier.loaded,
        });
        assert!(active.active.contains(&RegionId(2)));
    }
    #[test]
    fn linked_hold_thaw_horizon_overflow_is_atomic() {
        let mut game = fixture(2);
        let id = game
            .apply_linked_hold(ActorId(1), ActorId(2), policy())
            .unwrap()
            .unwrap();
        game.escape_linked_hold(ActorId(2), id).unwrap();
        game.actors.get_mut(&ActorId(2)).unwrap().combat = None;
        let runner = game
            .spawn_actor(at(3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut records = MemoryRecords::default();
        let frozen = RegionTransition {
            active: BTreeSet::from([RegionId(3)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
        };
        game.apply_region_transition(&frozen, &mut records).unwrap();
        game.tick = u64::MAX - 100;
        game.actors.get_mut(&runner).unwrap().ready_at = game.tick;
        let before = game.clone();
        let retained = records.len();
        assert_eq!(
            game.transition_regions_counted_deferred(
                &RegionTransition {
                    active: frozen.loaded.clone(),
                    loaded: frozen.loaded
                },
                &mut records
            ),
            Err(crate::TransitionError::Rule(GameError::Schedule(
                crate::ScheduleError::TimeExhausted
            )))
        );
        assert_eq!(game, before);
        assert_eq!(records.len(), retained);
    }
}
