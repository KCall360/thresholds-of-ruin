//! Data-driven, bounded timed conditions. Applications are keyed by condition
//! and, when requested by its definition, their causer.
use crate::creatures::SourceId;
use crate::{
    content_ids::ConditionId,
    rules::{ConditionDef, ConditionLifecycle, Eviction, Modifier, Ruleset, Stacking},
    ActorId,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionError {
    UnknownCondition,
    InvalidSource,
    InvalidDuration,
    InvalidStacks,
    InvalidModifiers,
    Duplicate,
    Capacity,
    InvalidOrder,
    InvalidReservation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionUpdate {
    Applied,
    Refreshed,
    Stacked,
    Immune,
    Evicted,
    Reserved,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionInstance {
    pub condition: ConditionId,
    pub source: ActorId,
    pub remaining: u64,
    pub stacks: u16,
    pub order: u64,
    #[serde(deserialize_with = "read_reservation")]
    pub reserved_by: Option<ActorId>,
}
fn read_reservation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ActorId>, D::Error> {
    serde::Deserialize::deserialize(deserializer)
}

/// A claim captured and reserved inside one preparation transaction. These are
/// not reusable grants; resumed work retains its claim and RPC retries/cancels
/// remain keyed by the preparation's intention identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenReservation {
    pub holder: ActorId,
    pub condition: ConditionId,
    pub source: ActorId,
    pub order: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConditionState {
    entries: BTreeMap<(ConditionId, ActorId), ConditionInstance>,
}
impl ConditionState {
    pub fn instances(&self) -> impl Iterator<Item = &ConditionInstance> {
        self.entries.values()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn evict_oldest(&mut self) -> Option<ConditionInstance> {
        let key = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.reserved_by.is_none())
            .min_by_key(|(_, entry)| entry.order)
            .map(|(key, _)| key.clone())?;
        self.entries.remove(&key)
    }
    pub fn remaining(&self, id: &ConditionId, source: ActorId) -> Option<u64> {
        self.entries
            .get(&(id.clone(), source))
            .map(|entry| entry.remaining)
            .or_else(|| {
                self.entries
                    .get(&(id.clone(), ActorId(0)))
                    .map(|entry| entry.remaining)
            })
    }
    pub fn next_expiry_in(&self) -> Option<u64> {
        self.entries
            .values()
            .filter(|entry| entry.reserved_by.is_none())
            .map(|entry| entry.remaining)
            .min()
    }
    pub fn from_recorded(
        entries: Vec<ConditionInstance>,
        rules: &Ruleset,
    ) -> Result<Self, ConditionError> {
        if entries.len() > crate::limits::CONDITIONS {
            return Err(ConditionError::Capacity);
        }
        let mut state = Self::default();
        let mut orders = std::collections::BTreeSet::new();
        for entry in entries {
            let def = rules
                .condition(&entry.condition)
                .ok_or(ConditionError::UnknownCondition)?;
            validate(
                def,
                entry.source,
                entry.remaining.max(u64::from(entry.reserved_by.is_some())),
            )?;
            if entry
                .reserved_by
                .is_some_and(|actor| actor.0 == 0 || actor.0 == u64::MAX)
                || (entry.reserved_by.is_some() && def.lifecycle == ConditionLifecycle::Timed)
            {
                return Err(ConditionError::InvalidReservation);
            }
            let max = match def.stacking {
                Stacking::Stack { maximum } => maximum,
                _ => 1,
            };
            if entry.stacks == 0 || entry.stacks > max {
                return Err(ConditionError::InvalidStacks);
            }
            if !orders.insert(entry.order) {
                return Err(ConditionError::InvalidOrder);
            }
            let key = key(def, entry.source);
            if state.entries.insert(key, entry).is_some() {
                return Err(ConditionError::Duplicate);
            }
        }
        Ok(state)
    }
    pub fn apply(
        &mut self,
        def: &ConditionDef,
        source: ActorId,
        duration: u64,
        immune: bool,
    ) -> Result<ConditionUpdate, ConditionError> {
        validate(def, source, duration)?;
        let key = key(def, source);
        if self
            .entries
            .get(&key)
            .is_some_and(|entry| entry.reserved_by.is_some())
        {
            return Ok(ConditionUpdate::Reserved);
        }
        if immune {
            self.entries.retain(|(id, _), _| id != &def.id);
            return Ok(ConditionUpdate::Immune);
        }

        if let Some(entry) = self.entries.get_mut(&key) {
            let update = match def.stacking {
                Stacking::Refresh => {
                    entry.remaining = duration;
                    ConditionUpdate::Refreshed
                }
                Stacking::Longest => {
                    entry.remaining = entry.remaining.max(duration);
                    ConditionUpdate::Refreshed
                }
                Stacking::Stack { maximum } => {
                    entry.stacks = entry.stacks.saturating_add(1).min(maximum);
                    entry.remaining = entry.remaining.max(duration);
                    ConditionUpdate::Stacked
                }
            };
            return Ok(update);
        }
        if self.entries.values().any(|entry| entry.order == u64::MAX)
            && self
                .entries
                .values()
                .any(|entry| entry.reserved_by.is_some())
        {
            return Err(ConditionError::InvalidOrder);
        }
        let mut update = ConditionUpdate::Applied;
        if self.entries.len() >= crate::limits::CONDITIONS {
            match def.capacity {
                Eviction::Reject => return Err(ConditionError::Capacity),
                Eviction::Oldest => {
                    if let Some(oldest) = self
                        .entries
                        .iter()
                        .filter(|(_, entry)| entry.reserved_by.is_none())
                        .min_by_key(|(_, entry)| entry.order)
                        .map(|(key, _)| key.clone())
                    {
                        self.entries.remove(&oldest);
                    } else {
                        return Err(ConditionError::Capacity);
                    }
                    update = ConditionUpdate::Evicted;
                }
            }
        }
        // Compact sequence numbers before overflow, preserving deterministic age.
        if self.entries.values().any(|entry| entry.order == u64::MAX) {
            if self
                .entries
                .values()
                .any(|entry| entry.reserved_by.is_some())
            {
                return Err(ConditionError::InvalidOrder);
            }
            let mut keys: Vec<_> = self
                .entries
                .iter()
                .map(|(key, entry)| (entry.order, key.clone()))
                .collect();
            keys.sort();
            for (order, (_, key)) in keys.into_iter().enumerate() {
                if let Some(entry) = self.entries.get_mut(&key) {
                    entry.order = order as u64;
                }
            }
        }
        let order = self
            .entries
            .values()
            .map(|entry| entry.order)
            .max()
            .map_or(0, |n| n + 1);
        self.entries.insert(
            key,
            ConditionInstance {
                condition: def.id.clone(),
                source,
                remaining: duration,
                stacks: 1,
                order,
                reserved_by: None,
            },
        );
        Ok(update)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&ConditionId) -> bool) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|(id, _), entry| entry.reserved_by.is_some() || keep(id));
        self.entries.len() != before
    }
    pub fn advance_active(&mut self, ticks: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            entry.remaining = entry.remaining.saturating_sub(ticks);
            entry.remaining > 0 || entry.reserved_by.is_some()
        });
        before != self.entries.len()
    }
    pub(crate) fn qualifying_tokens(
        &self,
        rules: &Ruleset,
        holder: ActorId,
        incoming: bool,
    ) -> Vec<TokenReservation> {
        let lifecycle = if incoming {
            ConditionLifecycle::NextCheckAgainst
        } else {
            ConditionLifecycle::NextCheck
        };
        self.entries
            .values()
            .filter(|entry| {
                entry.reserved_by.is_none()
                    && rules
                        .condition(&entry.condition)
                        .is_some_and(|def| def.lifecycle == lifecycle)
            })
            .map(|entry| TokenReservation {
                holder,
                condition: entry.condition.clone(),
                source: entry.source,
                order: entry.order,
            })
            .collect()
    }
    pub(crate) fn reserve(
        &mut self,
        token: &TokenReservation,
        owner: ActorId,
        rules: &Ruleset,
    ) -> Result<bool, ConditionError> {
        if rules
            .condition(&token.condition)
            .is_none_or(|def| def.lifecycle == ConditionLifecycle::Timed)
        {
            return Err(ConditionError::InvalidReservation);
        }
        if owner.0 == 0 || owner.0 == u64::MAX {
            return Err(ConditionError::InvalidReservation);
        }
        let entry = self
            .entries
            .values_mut()
            .find(|entry| {
                entry.condition == token.condition
                    && entry.source == token.source
                    && entry.order == token.order
            })
            .ok_or(ConditionError::InvalidReservation)?;
        match entry.reserved_by {
            Some(previous) if previous == owner => Ok(false),
            Some(_) => Err(ConditionError::InvalidReservation),
            None if entry.remaining > 0 => {
                entry.reserved_by = Some(owner);
                Ok(true)
            }
            None => Err(ConditionError::InvalidReservation),
        }
    }
    pub(crate) fn consume_reservation(&mut self, token: &TokenReservation, owner: ActorId) -> bool {
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            !(entry.condition == token.condition
                && entry.source == token.source
                && entry.order == token.order
                && entry.reserved_by == Some(owner))
        });
        self.entries.len() != before
    }

    pub fn sources(&self, rules: &Ruleset) -> BTreeMap<SourceId, Vec<Modifier>> {
        self.entries
            .values()
            .filter(|entry| entry.reserved_by.is_none())
            .filter_map(|entry| {
                rules.condition(&entry.condition).map(|def| {
                    (
                        SourceId::Condition(entry.condition.clone(), entry.source),
                        (0..entry.stacks)
                            .flat_map(|_| def.modifiers.clone())
                            .collect(),
                    )
                })
            })
            .collect()
    }
}
fn key(def: &ConditionDef, source: ActorId) -> (ConditionId, ActorId) {
    (
        def.id.clone(),
        if def.source_scoped {
            source
        } else {
            ActorId(0)
        },
    )
}
fn validate(_: &ConditionDef, source: ActorId, duration: u64) -> Result<(), ConditionError> {
    if source.0 == 0 || source.0 == u64::MAX {
        return Err(ConditionError::InvalidSource);
    }
    if duration == 0 || duration > crate::limits::NUMERIC as u64 {
        return Err(ConditionError::InvalidDuration);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::DEFAULT_RULESET_TOML;

    #[test]
    fn reserved_tokens_cannot_be_reused_refreshed_or_evicted_and_expiry_keeps_the_hold() {
        let rules = Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let def = rules
            .condition(&ConditionId::new("readied").unwrap())
            .unwrap();
        let mut state = ConditionState::default();
        state.apply(def, ActorId(7), 40, false).unwrap();
        let token = state
            .qualifying_tokens(&rules, ActorId(1), false)
            .pop()
            .unwrap();
        assert_eq!(state.reserve(&token, ActorId(2), &rules), Ok(true));
        let held = state.clone();
        assert_eq!(
            state.reserve(&token, ActorId(3), &rules),
            Err(ConditionError::InvalidReservation)
        );
        assert_eq!(state, held);
        assert_eq!(
            state.apply(def, ActorId(8), 100, false),
            Ok(ConditionUpdate::Reserved)
        );
        assert_eq!(state, held);
        assert!(state.evict_oldest().is_none());
        state.advance_active(80);
        assert_eq!(state.instances().next().unwrap().remaining, 0);
        assert!(state.next_expiry_in().is_none());
        assert!(state.sources(&rules).is_empty());
        let restored =
            ConditionState::from_recorded(state.instances().cloned().collect(), &rules).unwrap();
        assert_eq!(restored, state);
        assert!(state.consume_reservation(&token, ActorId(2)));
        assert!(state.is_empty());
        assert!(!state.consume_reservation(&token, ActorId(2)));
    }
}
