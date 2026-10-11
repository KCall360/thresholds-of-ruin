//! Data-driven, bounded timed conditions. Applications are keyed by condition
//! and, when requested by its definition, their causer.
use crate::creatures::SourceId;
use crate::{
    content_ids::ConditionId,
    rules::{ConditionDef, Eviction, Modifier, Ruleset, Stacking},
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionUpdate {
    Applied,
    Refreshed,
    Stacked,
    Immune,
    Evicted,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionInstance {
    pub condition: ConditionId,
    pub source: ActorId,
    pub remaining: u64,
    pub stacks: u16,
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
        self.entries.values().map(|entry| entry.remaining).min()
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
            validate(def, entry.source, entry.remaining)?;
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
        if immune {
            self.entries.retain(|(id, _), _| id != &def.id);
            return Ok(ConditionUpdate::Immune);
        }
        let key = key(def, source);
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
        let mut update = ConditionUpdate::Applied;
        if self.entries.len() >= crate::limits::CONDITIONS {
            match def.capacity {
                Eviction::Reject => return Err(ConditionError::Capacity),
                Eviction::Oldest => {
                    if let Some(oldest) = self
                        .entries
                        .iter()
                        .min_by_key(|(_, entry)| entry.order)
                        .map(|(key, _)| key.clone())
                    {
                        self.entries.remove(&oldest);
                    }
                    update = ConditionUpdate::Evicted;
                }
            }
        }
        // Compact sequence numbers before overflow, preserving deterministic age.
        if self.entries.values().any(|entry| entry.order == u64::MAX) {
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
            },
        );
        Ok(update)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&ConditionId) -> bool) -> bool {
        let before = self.entries.len();
        self.entries.retain(|(id, _), _| keep(id));
        self.entries.len() != before
    }
    pub fn advance_active(&mut self, ticks: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|_, entry| {
            entry.remaining = entry.remaining.saturating_sub(ticks);
            entry.remaining > 0
        });
        before != self.entries.len()
    }
    pub fn sources(&self, rules: &Ruleset) -> BTreeMap<SourceId, Vec<Modifier>> {
        self.entries
            .values()
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
