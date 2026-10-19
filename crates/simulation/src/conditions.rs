//! Data-driven, bounded timed conditions. Applications are keyed by condition
//! and, when requested by its definition, their causer.
use crate::creatures::SourceId;
use crate::{
    content_ids::{AbilityId, AbilityTagId, ConditionId},
    rules::{
        ConditionDef, ConditionLifecycle, ConditionPreparation, ConditionRestriction,
        ConditionTicks, Eviction, Modifier, Ruleset, Stacking,
    },
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
    InvalidQualification,
    InvalidPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionUpdate {
    Applied,
    Refreshed,
    Stacked,
    Immune,
    Evicted,
    Reserved,
    Unchanged,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionPhase {
    Active,
    Protection,
}
impl<'de> serde::Deserialize<'de> for ConditionPhase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match <String as serde::Deserialize>::deserialize(deserializer)?.as_str() {
            "active" => Ok(Self::Active),
            "protection" => Ok(Self::Protection),
            _ => Err(serde::de::Error::custom("Unknown condition phase")),
        }
    }
}
/// Additional eligibility for one outgoing token, without a second family or
/// condition instance. The actor identity never belongs in ordinary wire views.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TokenQualification {
    pub opponent: ActorId,
    pub ability_tag: AbilityTagId,
}
impl<'de> serde::Deserialize<'de> for TokenQualification {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            opponent: ActorId,
            ability_tag: AbilityTagId,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            opponent: fields.opponent,
            ability_tag: fields.ability_tag,
        })
    }
}
impl TokenQualification {
    pub(crate) fn valid(&self, definition: &ConditionDef, rules: &Ruleset) -> bool {
        definition.supports_qualification()
            && self.opponent.0 > 0
            && self.opponent.0 != u64::MAX
            && rules.ability_tag(&self.ability_tag).is_some()
    }
    pub(crate) fn matches(
        &self,
        opponent: ActorId,
        ability: Option<&AbilityId>,
        rules: &Ruleset,
    ) -> bool {
        self.opponent == opponent
            && ability
                .and_then(|id| rules.ability(id))
                .is_some_and(|definition| definition.tags.contains(&self.ability_tag))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionInstance {
    pub condition: ConditionId,
    pub phase: ConditionPhase,
    pub source: ActorId,
    pub remaining: u64,
    pub stacks: u16,
    pub order: u64,
    #[serde(deserialize_with = "read_reservation")]
    pub reserved_by: Option<ActorId>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub qualification: Option<TokenQualification>,
}
impl<'de> serde::Deserialize<'de> for ConditionInstance {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            condition: ConditionId,
            phase: ConditionPhase,
            source: ActorId,
            remaining: u64,
            stacks: u16,
            order: u64,
            #[serde(deserialize_with = "read_reservation")]
            reserved_by: Option<ActorId>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            qualification: Option<TokenQualification>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            condition: fields.condition,
            phase: fields.phase,
            source: fields.source,
            remaining: fields.remaining,
            stacks: fields.stacks,
            order: fields.order,
            reserved_by: fields.reserved_by,
            qualification: fields.qualification,
        })
    }
}
fn read_reservation<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ActorId>, D::Error> {
    serde::Deserialize::deserialize(deserializer)
}

/// A claim captured and reserved inside one preparation transaction. These are
/// not reusable grants; resumed work retains its claim and RPC retries/cancels
/// remain keyed by the preparation's intention identity.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TokenReservation {
    pub holder: ActorId,
    pub condition: ConditionId,
    pub source: ActorId,
    pub order: u64,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub qualification: Option<TokenQualification>,
}
impl<'de> serde::Deserialize<'de> for TokenReservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            holder: ActorId,
            condition: ConditionId,
            source: ActorId,
            order: u64,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            qualification: Option<TokenQualification>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            holder: fields.holder,
            condition: fields.condition,
            source: fields.source,
            order: fields.order,
            qualification: fields.qualification,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConditionState {
    entries: BTreeMap<(ConditionId, ActorId), ConditionEntry>,
}
/// Immutable definition-derived metadata stays beside its bounded book entry.
/// Only the instance is saved; restore re-derives this metadata from pinned rules.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConditionEntry {
    instance: ConditionInstance,
    protection: Option<ConditionTicks>,
    protected: bool,
}
impl ConditionEntry {
    fn new(instance: ConditionInstance, definition: &ConditionDef) -> Self {
        Self {
            instance,
            protection: definition.timing.protection(),
            protected: definition.protected_lifecycle(),
        }
    }
}
impl std::ops::Deref for ConditionEntry {
    type Target = ConditionInstance;
    fn deref(&self) -> &Self::Target {
        &self.instance
    }
}
impl std::ops::DerefMut for ConditionEntry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.instance
    }
}
/// Gameplay endings share the follow-up transition. Administrative replacement
/// and death remove entries directly and deliberately do not call this helper.
enum ConditionEnd {
    Expiry { elapsed_after: u64 },
    Immunity,
    Removal,
}
fn end_active(entry: &mut ConditionEntry, reason: ConditionEnd) -> bool {
    if entry.phase == ConditionPhase::Protection {
        return true;
    }
    let Some(duration) = entry.protection else {
        return false;
    };
    let elapsed_after = match reason {
        ConditionEnd::Expiry { elapsed_after } => elapsed_after,
        ConditionEnd::Immunity | ConditionEnd::Removal => 0,
    };
    entry.phase = ConditionPhase::Protection;
    entry.remaining = duration.get().saturating_sub(elapsed_after);
    entry.remaining > 0
}
impl ConditionState {
    pub fn instances(&self) -> impl Iterator<Item = &ConditionInstance> {
        self.entries.values().map(|entry| &entry.instance)
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// End active applications through their normal expiry policy. Existing
    /// protection is retained, and paid token reservations remain tombstones.
    pub(crate) fn remove(&mut self, condition: &ConditionId) -> bool {
        let mut changed = false;
        self.entries.retain(|(id, _), entry| {
            if id != condition || entry.phase != ConditionPhase::Active || entry.remaining == 0 {
                return true;
            }
            changed = true;
            if entry.reserved_by.is_some() {
                entry.remaining = 0;
                true
            } else {
                end_active(entry, ConditionEnd::Removal)
            }
        });
        changed
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    pub fn evict_oldest(&mut self) -> Option<ConditionInstance> {
        let key = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.reserved_by.is_none() && !entry.protected)
            .min_by_key(|(_, entry)| entry.order)
            .map(|(key, _)| key.clone())?;
        self.entries.remove(&key).map(|entry| entry.instance)
    }
    /// Administrative replacement is checked while both definitions exist.
    /// Reserved tombstones still belong to a paid plan, so incompatible content
    /// rejects replacement rather than reinterpreting or retiring that claim.
    pub(crate) fn reconcile_definitions(
        &mut self,
        previous: &Ruleset,
        next: &Ruleset,
    ) -> Result<bool, ConditionError> {
        let unchanged = |id: &ConditionId| {
            previous
                .condition(id)
                .zip(next.condition(id))
                .is_some_and(|(old, new)| old.same_mechanics(new))
        };
        if self
            .entries
            .values()
            .any(|entry| entry.reserved_by.is_some() && !unchanged(&entry.condition))
        {
            return Err(ConditionError::InvalidReservation);
        }
        Ok(self.retain(unchanged))
    }
    /// Called only after metadata reconciliation against validated definitions.
    /// False proves there are no live condition modifiers, including unexpired
    /// reserved tokens. Otherwise the seed is necessarily ordinary and evictable:
    /// protected definitions are modifier-free and reserved entries do not fold.
    /// Choosing any older eligible entry preserves ordinary oldest-first policy.
    pub(crate) fn evict_for_reconciliation(&mut self, rules: &Ruleset) -> bool {
        let Some((mut key, mut order)) = self.entries.iter().find_map(|(key, entry)| {
            (entry.phase == ConditionPhase::Active
                && entry.reserved_by.is_none()
                && rules
                    .condition(&entry.condition)
                    .is_some_and(|definition| !definition.modifiers.is_empty()))
            .then(|| (key.clone(), entry.order))
        }) else {
            return false;
        };
        for (candidate, entry) in &self.entries {
            if entry.reserved_by.is_none() && !entry.protected && entry.order < order {
                key = candidate.clone();
                order = entry.order;
            }
        }
        // The exact key came from this unchanged, exclusively borrowed map.
        self.entries.remove(&key);
        true
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
    pub(crate) fn last_boundary_in(&self) -> Option<u64> {
        self.entries
            .values()
            .filter(|entry| entry.reserved_by.is_none())
            .map(|entry| {
                entry.remaining
                    + if entry.phase == ConditionPhase::Active {
                        entry.protection.map_or(0, ConditionTicks::get)
                    } else {
                        0
                    }
            })
            .max()
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
            if entry
                .qualification
                .as_ref()
                .is_some_and(|scope| !scope.valid(def, rules))
            {
                return Err(ConditionError::InvalidQualification);
            }
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
            if entry.phase == ConditionPhase::Protection
                && (def
                    .timing
                    .protection()
                    .is_none_or(|limit| entry.remaining > limit.get())
                    || entry.reserved_by.is_some()
                    || entry.qualification.is_some())
            {
                return Err(ConditionError::InvalidPhase);
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
            if state
                .entries
                .insert(key, ConditionEntry::new(entry, def))
                .is_some()
            {
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
        self.apply_qualified(def, source, duration, immune, None)
    }
    pub(crate) fn apply_qualified(
        &mut self,
        def: &ConditionDef,
        source: ActorId,
        duration: u64,
        immune: bool,
        qualification: Option<TokenQualification>,
    ) -> Result<ConditionUpdate, ConditionError> {
        validate(def, source, duration)?;
        if qualification.as_ref().is_some_and(|scope| {
            !def.supports_qualification() || scope.opponent.0 == 0 || scope.opponent.0 == u64::MAX
        }) {
            return Err(ConditionError::InvalidQualification);
        }
        let key = key(def, source);
        if self
            .entries
            .get(&key)
            .is_some_and(|entry| entry.reserved_by.is_some())
        {
            return Ok(ConditionUpdate::Reserved);
        }
        if self
            .entries
            .get(&key)
            .is_some_and(|entry| entry.phase == ConditionPhase::Protection)
        {
            return Ok(ConditionUpdate::Immune);
        }
        if immune {
            self.entries.retain(|(id, _), entry| {
                id != &def.id || end_active(entry, ConditionEnd::Immunity)
            });
            return Ok(ConditionUpdate::Immune);
        }

        if let Some(entry) = self.entries.get_mut(&key) {
            if def.stacking == Stacking::Keep {
                return Ok(ConditionUpdate::Unchanged);
            }
            // Bound extension before changing any field, including qualification.
            let extended = if def.stacking == Stacking::Extend {
                Some(
                    entry
                        .remaining
                        .checked_add(duration)
                        .filter(|ticks| *ticks <= crate::limits::NUMERIC as u64)
                        .ok_or(ConditionError::InvalidDuration)?,
                )
            } else {
                None
            };
            entry.qualification = qualification;
            let update = match def.stacking {
                Stacking::Keep => unreachable!("handled before mutation"),
                Stacking::Refresh => {
                    entry.remaining = duration;
                    ConditionUpdate::Refreshed
                }
                Stacking::Extend => {
                    entry.remaining = extended.expect("validated extension");
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
                        .filter(|(_, entry)| entry.reserved_by.is_none() && !entry.protected)
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
            ConditionEntry::new(
                ConditionInstance {
                    condition: def.id.clone(),
                    phase: ConditionPhase::Active,
                    source,
                    remaining: duration,
                    stacks: 1,
                    order,
                    reserved_by: None,
                    qualification,
                },
                def,
            ),
        );
        Ok(update)
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&ConditionId) -> bool) -> bool {
        let before = self.entries.len();
        self.entries
            .retain(|(id, _), entry| entry.reserved_by.is_some() || keep(id));
        self.entries.len() != before
    }
    pub(crate) fn reconcile_immunity(
        &mut self,
        rules: &Ruleset,
        mut immune: impl FnMut(&ConditionId) -> bool,
    ) -> bool {
        let mut changed = false;
        self.entries.retain(|(id, _), entry| {
            if entry.reserved_by.is_some() {
                return true;
            }
            let Some(definition) = rules.condition(id) else {
                changed = true;
                return false;
            };
            // Changed/removed definitions are an administrative replacement,
            // not a gameplay end that can grant a new protection window.
            if entry.protection != definition.timing.protection()
                || entry.protected != definition.protected_lifecycle()
            {
                changed = true;
                return false;
            }
            if entry.phase == ConditionPhase::Protection || !immune(id) {
                return true;
            }
            changed = true;
            end_active(entry, ConditionEnd::Immunity)
        });
        changed
    }
    pub fn advance_active(&mut self, ticks: u64) -> bool {
        let mut changed = false;
        self.entries.retain(|_, entry| {
            if ticks < entry.remaining || entry.reserved_by.is_some() {
                entry.remaining = entry.remaining.saturating_sub(ticks);
                return true;
            }
            changed = true;
            let beyond = ticks - entry.remaining;
            if entry.phase == ConditionPhase::Active {
                return end_active(
                    entry,
                    ConditionEnd::Expiry {
                        elapsed_after: beyond,
                    },
                );
            }
            false
        });
        changed
    }
    pub(crate) fn restricts(&self, rules: &Ruleset, operation: ConditionRestriction) -> bool {
        self.restricts_after(rules, operation, 0)
    }
    pub(crate) fn restricts_after(
        &self,
        rules: &Ruleset,
        operation: ConditionRestriction,
        elapsed: u64,
    ) -> bool {
        self.entries.values().any(|entry| {
            entry.phase == ConditionPhase::Active
                && entry.remaining > elapsed
                && entry.reserved_by.is_none()
                && rules
                    .condition(&entry.condition)
                    .is_some_and(|def| def.restrictions.contains(&operation))
        })
    }
    pub(crate) fn pauses_preparation(&self, rules: &Ruleset, action: &crate::Action) -> bool {
        self.entries.values().any(|entry| {
            entry.phase == ConditionPhase::Active
                && entry.reserved_by.is_none()
                && rules.condition(&entry.condition).is_some_and(|def| {
                    def.preparation == ConditionPreparation::Pause
                        && def
                            .restrictions
                            .iter()
                            .any(|operation| operation.blocks(action))
                })
        })
    }
    pub(crate) fn qualifying_tokens(
        &self,
        rules: &Ruleset,
        holder: ActorId,
        incoming: bool,
    ) -> Vec<TokenReservation> {
        self.qualifying_tokens_for(rules, holder, incoming, None)
    }
    pub(crate) fn qualifying_tokens_for(
        &self,
        rules: &Ruleset,
        holder: ActorId,
        incoming: bool,
        check: Option<(ActorId, &AbilityId, crate::rules::CheckActor)>,
    ) -> Vec<TokenReservation> {
        let lifecycle = if incoming {
            ConditionLifecycle::NextCheckAgainst
        } else {
            ConditionLifecycle::NextCheck
        };
        self.entries
            .values()
            .filter(|entry| {
                entry.phase == ConditionPhase::Active
                    && entry.reserved_by.is_none()
                    && entry.qualification.as_ref().is_none_or(|scope| {
                        !incoming
                            && check.is_some_and(|(opponent, ability, role)| {
                                role == crate::rules::CheckActor::User
                                    && scope.matches(opponent, Some(ability), rules)
                            })
                    })
                    && rules
                        .condition(&entry.condition)
                        .is_some_and(|def| def.lifecycle == lifecycle)
            })
            .map(|entry| TokenReservation {
                holder,
                condition: entry.condition.clone(),
                source: entry.source,
                order: entry.order,
                qualification: entry.qualification.clone(),
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
                    && entry.qualification == token.qualification
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
                && entry.qualification == token.qualification
                && entry.reserved_by == Some(owner))
        });
        self.entries.len() != before
    }

    pub fn sources(&self, rules: &Ruleset) -> BTreeMap<SourceId, Vec<Modifier>> {
        self.entries
            .values()
            .filter(|entry| entry.phase == ConditionPhase::Active && entry.reserved_by.is_none())
            .filter_map(|entry| {
                rules.condition(&entry.condition).map(|def| {
                    (
                        entry.qualification.as_ref().map_or_else(
                            || SourceId::Condition(entry.condition.clone(), entry.source),
                            |qualification| SourceId::QualifiedCondition {
                                condition: entry.condition.clone(),
                                source: entry.source,
                                qualification: qualification.clone(),
                            },
                        ),
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
    ConditionTicks::new(duration)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::DEFAULT_RULESET_TOML;

    #[test]
    fn extend_duration_preserves_one_application_and_expires_without_protection() {
        let rules = crate::test_creatures::ruleset();
        let mut definition = rules
            .condition(&ConditionId::new("readied").unwrap())
            .unwrap()
            .clone();
        definition.stacking = Stacking::Extend;
        definition.lifecycle = ConditionLifecycle::Timed;
        let mut state = ConditionState::default();
        assert_eq!(
            state.apply(&definition, ActorId(1), 1900, false),
            Ok(ConditionUpdate::Applied)
        );
        state.advance_active(100);
        assert_eq!(
            state.apply(&definition, ActorId(2), 2000, false),
            Ok(ConditionUpdate::Refreshed)
        );
        assert_eq!(state.instances().count(), 1);
        let entry = state.instances().next().unwrap();
        assert_eq!(
            (entry.remaining, entry.stacks, entry.source),
            (3800, 1, ActorId(1))
        );
        state.advance_active(3799);
        assert_eq!(state.instances().next().unwrap().remaining, 1);
        state.advance_active(1);
        assert!(state.is_empty());
        assert_eq!(
            state.apply(&definition, ActorId(2), 1900, false),
            Ok(ConditionUpdate::Applied)
        );
    }

    #[test]
    fn extend_duration_rejects_timer_overflow_without_partial_mutation() {
        let rules = crate::test_creatures::ruleset();
        let mut definition = rules
            .condition(&ConditionId::new("readied").unwrap())
            .unwrap()
            .clone();
        definition.stacking = Stacking::Extend;
        let mut state = ConditionState::default();
        state
            .apply(
                &definition,
                ActorId(1),
                crate::limits::NUMERIC as u64,
                false,
            )
            .unwrap();
        let before = state.clone();
        assert_eq!(
            state.apply(&definition, ActorId(2), 1, false),
            Err(ConditionError::InvalidDuration)
        );
        assert_eq!(state, before);
    }

    #[test]
    fn removal_preserves_other_conditions_protection_and_paid_reservation_tombstones() {
        let rules = crate::test_creatures::ruleset();
        let ordinary = rules
            .condition(&ConditionId::new("readied").unwrap())
            .unwrap()
            .clone();
        let mut protected = ordinary.clone();
        protected.id = ConditionId::new("fixture_protected").unwrap();
        protected.lifecycle = ConditionLifecycle::Timed;
        protected.timing = crate::rules::ConditionTiming::ActorActive {
            on_expiry: crate::rules::ConditionExpiry::Protect {
                duration: ConditionTicks::new(700).unwrap(),
            },
        };
        let mut state = ConditionState::default();
        state.apply(&ordinary, ActorId(1), 300, false).unwrap();
        state.apply(&protected, ActorId(2), 100, false).unwrap();
        assert!(state.remove(&protected.id));
        assert!(!state.remove(&protected.id));
        let entry = state
            .instances()
            .find(|entry| entry.condition == protected.id)
            .unwrap();
        assert_eq!(
            (entry.phase, entry.remaining),
            (ConditionPhase::Protection, 700)
        );
        assert_eq!(
            state
                .instances()
                .find(|entry| entry.condition == ordinary.id)
                .unwrap()
                .remaining,
            300
        );
        let token = state
            .qualifying_tokens(&rules, ActorId(1), false)
            .pop()
            .unwrap();
        state.reserve(&token, ActorId(3), &rules).unwrap();
        assert!(state.remove(&ordinary.id));
        let entry = state
            .instances()
            .find(|entry| entry.condition == ordinary.id)
            .unwrap();
        assert_eq!((entry.remaining, entry.reserved_by), (0, Some(ActorId(3))));
        assert!(state.consume_reservation(&token, ActorId(3)));
        assert_eq!(state.instances().count(), 1);
    }

    #[test]
    fn qualified_readied_filters_contribution_and_reservation_before_family_deduplication() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let rules = build.ruleset().clone();
        let mut creature = crate::creatures::CreatureState::new(build).unwrap();
        let id = ConditionId::new("readied").unwrap();
        let attack = AbilityId::new("basic_melee").unwrap();
        let nonattack = AbilityId::new("falter").unwrap();
        let opponent = ActorId(2);
        let scope = TokenQualification {
            opponent,
            ability_tag: AbilityTagId::new("weapon_attack").unwrap(),
        };
        creature
            .apply_qualified_condition(&id, opponent, 300, Some(scope.clone()))
            .unwrap();
        for (target, ability, expected) in [
            (opponent, &attack, 1),
            (ActorId(3), &attack, 0),
            (opponent, &nonattack, 0),
        ] {
            assert_eq!(
                creature
                    .derived()
                    .check_edge(&rules, target, ability)
                    .unwrap(),
                expected
            );
            let selected = creature.conditions().qualifying_tokens_for(
                &rules,
                ActorId(1),
                false,
                Some((target, ability, crate::rules::CheckActor::User)),
            );
            assert_eq!(selected.len(), expected as usize);
            assert_eq!(
                creature
                    .conditions()
                    .instances()
                    .next()
                    .unwrap()
                    .reserved_by,
                None
            );
        }
        assert_eq!(creature.derived().edge_against(opponent).balance(), 0);
        assert_eq!(
            creature
                .derived()
                .incoming_check_edge(&rules, opponent, None)
                .unwrap(),
            0
        );
        let token = creature
            .conditions()
            .qualifying_tokens_for(
                &rules,
                ActorId(1),
                false,
                Some((opponent, &attack, crate::rules::CheckActor::User)),
            )
            .pop()
            .unwrap();
        creature
            .reserve_tokens(std::slice::from_ref(&token), ActorId(1))
            .unwrap();
        assert_eq!(
            creature
                .derived()
                .check_edge(&rules, opponent, &attack)
                .unwrap(),
            0
        );
        assert_eq!(
            creature
                .apply_qualified_condition(
                    &id,
                    ActorId(3),
                    500,
                    Some(TokenQualification {
                        opponent: ActorId(3),
                        ..scope.clone()
                    })
                )
                .unwrap(),
            ConditionUpdate::Reserved
        );
        assert_eq!(
            creature
                .conditions()
                .instances()
                .next()
                .unwrap()
                .qualification,
            Some(scope)
        );
        creature.advance_active(301);
        assert_eq!(
            creature.conditions().instances().next().unwrap().remaining,
            0
        );
        creature.consume_tokens(&[token], ActorId(1));
        assert!(creature.conditions().is_empty());
    }

    #[test]
    fn unreserved_readied_renewal_replaces_or_clears_one_instance_qualification() {
        let rules = Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let def = rules
            .condition(&ConditionId::new("readied").unwrap())
            .unwrap();
        let mut state = ConditionState::default();
        for opponent in [ActorId(2), ActorId(3)] {
            state
                .apply_qualified(
                    def,
                    opponent,
                    300,
                    false,
                    Some(TokenQualification {
                        opponent,
                        ability_tag: AbilityTagId::new("weapon_attack").unwrap(),
                    }),
                )
                .unwrap();
            assert_eq!(state.instances().count(), 1);
            assert_eq!(
                state
                    .instances()
                    .next()
                    .unwrap()
                    .qualification
                    .as_ref()
                    .unwrap()
                    .opponent,
                opponent
            );
        }
        state.apply(def, ActorId(1), 200, false).unwrap();
        assert_eq!(state.instances().count(), 1);
        assert!(state.instances().next().unwrap().qualification.is_none());
        // The ordinary Readied definition retains Longest renewal semantics.
        assert_eq!(state.instances().next().unwrap().remaining, 300);
    }

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
