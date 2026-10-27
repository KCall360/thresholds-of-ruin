//! Immutable Hit Dice records and reversible, current-composition derivation.
//!
//! The ledger never consumes combat randomness. Each record owns a separate
//! health seed, interpreted against its current die by deterministic sampling.

use crate::dice::{die, random_word, random_word_at};

use crate::content_ids::{ClassId, TypeId};
use crate::rules::Ruleset;
use crate::HdCount;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum HdSource {
    Racial,
    Class(ClassId),
}

/// Cached derivation facts for one owned hit die. Never part of a save record.
#[derive(Debug, PartialEq, Eq)]
pub struct HealthContribution {
    pub die: u16,
    pub health: u32,
}

impl Clone for HealthContribution {
    fn clone(&self) -> Self {
        crate::diagnostics::health_provenance_copy();
        Self {
            die: self.die,
            health: self.health,
        }
    }
}

/// Immutable acquisition-time source interpretation. Later templates do not
/// expand this owned roll's improvement budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HealthOrigin {
    pub templates: Vec<crate::content_ids::TemplateId>,
    pub kind: TypeId,
    pub die: u16,
}
/// Acquisition-owned Health interpretation. Composition never changes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthRollPolicy {
    Maximized,
    Seeded,
}
impl HealthRollPolicy {
    fn for_ordinal(ordinal: usize) -> Self {
        if ordinal == 0 {
            Self::Maximized
        } else {
            Self::Seeded
        }
    }
    fn maximized(self) -> bool {
        self == Self::Maximized
    }
}
/// Exact one-point owned-record mutation selected from compiled active facts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct HitDieHealthImprovement {
    pub ordinal: crate::HdIndex,
    pub source: HdSource,
    pub health_seed: u64,
    pub health_policy: HealthRollPolicy,
    pub original_kind: TypeId,
    pub original_die: u16,
    pub previous: u16,
}
impl<'de> serde::Deserialize<'de> for HitDieHealthImprovement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            ordinal: crate::HdIndex,
            #[serde(deserialize_with = "crate::bounded::named_map")]
            source: HdSource,
            health_seed: u64,
            health_policy: HealthRollPolicy,
            original_kind: TypeId,
            original_die: u16,
            previous: u16,
        }
        let f: Fields = crate::bounded::named_map(deserializer)?;
        let claim = Self {
            ordinal: f.ordinal,
            source: f.source,
            health_seed: f.health_seed,
            health_policy: f.health_policy,
            original_kind: f.original_kind,
            original_die: f.original_die,
            previous: f.previous,
        };
        if !claim.valid() {
            return Err(serde::de::Error::custom("invalid owned Health improvement"));
        }
        Ok(claim)
    }
}
impl HitDieHealthImprovement {
    pub(crate) fn valid(&self) -> bool {
        self.ordinal.get() < crate::limits::HIT_DICE as u64
            && (2..=crate::limits::DICE_SIDES).contains(&self.original_die)
            && self.previous
                < self.original_die
                    - original_roll(
                        self.health_seed,
                        self.original_die,
                        self.health_policy.maximized(),
                    )
    }
}
fn original_roll(seed: u64, sides: u16, first: bool) -> u16 {
    if first {
        sides
    } else {
        let mut state = seed;
        die(&mut state, sides)
    }
}
fn source_die(source: &HdSource, kind: &TypeId, rules: &Ruleset) -> Result<u16, HdError> {
    match source {
        HdSource::Racial => rules
            .creature_type(kind)
            .map(|definition| definition.health_die),
        HdSource::Class(class) => rules.class(class).map(|definition| definition.health_die),
    }
    .ok_or(HdError::UnknownSource)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitDie {
    source: HdSource,
    owner: Option<crate::creatures::composition::HdOwner>,
    health_seed: u64,
    // Only fresh unadmitted legacy rows can be unbound. All ledger rows are bound.
    health_policy: Option<HealthRollPolicy>,
    health_origin: Option<HealthOrigin>,
    health_improvement: u16,
}

impl HitDie {
    pub fn new(source: HdSource, health_seed: u64) -> Self {
        Self {
            source,
            owner: None,
            health_seed,
            health_policy: None,
            health_origin: None,
            health_improvement: 0,
        }
    }

    /// Construct a body/personal owned row with policy independent of its
    /// eventual effective position. Only the first body HD is maximized.
    pub fn with_health_policy(
        source: HdSource,
        health_seed: u64,
        policy: HealthRollPolicy,
    ) -> Self {
        let mut row = Self::new(source, health_seed);
        row.health_policy = Some(policy);
        row
    }
    /// Bind an uncompiled acquisition to its owner. Bound origin/improvement
    /// history cannot be reinterpreted as a newly maximized roll.
    pub fn with_health_roll_policy(mut self, policy: HealthRollPolicy) -> Result<Self, HdError> {
        if (self.health_origin.is_some() || self.owner.is_some())
            && self.health_policy != Some(policy)
        {
            return Err(HdError::InvalidHealthImprovement);
        }
        self.health_policy = Some(policy);
        Ok(self)
    }
    pub fn owner(&self) -> Option<&crate::creatures::composition::HdOwner> {
        self.owner.as_ref()
    }
    /// Attach stable provenance once. A package/body owner cannot turn an
    /// already acquired personal roll into a different funding or Health owner.
    pub fn with_owner(
        mut self,
        owner: crate::creatures::composition::HdOwner,
    ) -> Result<Self, HdError> {
        if self
            .owner
            .as_ref()
            .is_some_and(|existing| existing != &owner)
        {
            return Err(HdError::InvalidHealthImprovement);
        }
        let policy = if owner.maximizes_health() {
            HealthRollPolicy::Maximized
        } else {
            HealthRollPolicy::Seeded
        };
        self = self.with_health_roll_policy(policy)?;
        self.owner = Some(owner);
        Ok(self)
    }
    pub fn health_policy(&self) -> Option<HealthRollPolicy> {
        self.health_policy
    }
    fn bind_ordinal_policy(&mut self, ordinal: usize) {
        self.health_policy
            .get_or_insert(HealthRollPolicy::for_ordinal(ordinal));
    }

    pub fn source(&self) -> HdSource {
        self.source.clone()
    }

    pub fn health_seed(&self) -> u64 {
        self.health_seed
    }

    pub fn health_origin(&self) -> Option<&HealthOrigin> {
        self.health_origin.as_ref()
    }
    pub fn health_improvement(&self) -> u16 {
        self.health_improvement
    }
    #[cfg(test)]
    pub(crate) fn from_recorded(
        source: HdSource,
        health_seed: u64,
        origin: HealthOrigin,
        improvement: u16,
        ordinal: usize,
    ) -> Result<Self, HdError> {
        Self::from_recorded_with_policy(
            source,
            health_seed,
            origin,
            improvement,
            HealthRollPolicy::for_ordinal(ordinal),
        )
    }
    pub(crate) fn from_recorded_with_policy(
        source: HdSource,
        health_seed: u64,
        origin: HealthOrigin,
        improvement: u16,
        policy: HealthRollPolicy,
    ) -> Result<Self, HdError> {
        if !(2..=crate::limits::DICE_SIDES).contains(&origin.die)
            || improvement > origin.die - original_roll(health_seed, origin.die, policy.maximized())
        {
            return Err(HdError::InvalidHealthImprovement);
        }
        Ok(Self {
            source,
            owner: None,
            health_seed,
            health_policy: Some(policy),
            health_origin: Some(origin),
            health_improvement: improvement,
        })
    }
    fn bind_origin(
        &mut self,
        kind: &TypeId,
        templates: &[crate::content_ids::TemplateId],
        rules: &Ruleset,
    ) -> Result<(), HdError> {
        let policy = self
            .health_policy
            .ok_or(HdError::InvalidHealthImprovement)?;
        let origin = self.health_origin.get_or_insert(HealthOrigin {
            templates: templates.to_vec(),
            kind: kind.clone(),
            die: source_die(&self.source, kind, rules)?,
        });
        if source_die(&self.source, &origin.kind, rules)? != origin.die
            || self.health_improvement
                > origin.die - original_roll(self.health_seed, origin.die, policy.maximized())
        {
            return Err(HdError::InvalidHealthImprovement);
        }
        Ok(())
    }
    fn improvement_claim(&self, ordinal: usize) -> Option<HitDieHealthImprovement> {
        let origin = self.health_origin.as_ref()?;
        let claim = HitDieHealthImprovement {
            ordinal: crate::HdIndex(ordinal as u64),
            source: self.source(),
            health_seed: self.health_seed,
            health_policy: self.health_policy?,
            original_kind: origin.kind.clone(),
            original_die: origin.die,
            previous: self.health_improvement,
        };
        claim.valid().then_some(claim)
    }
    fn health(&self, kind: &TypeId, rules: &Ruleset) -> Result<HealthContribution, HdError> {
        crate::diagnostics::raw_hit_die_visit();
        crate::diagnostics::health_derivation();
        let sides = source_die(&self.source, kind, rules)?;
        let mut state = self.health_seed;
        Ok(HealthContribution {
            die: sides,
            health: if self
                .health_policy
                .ok_or(HdError::InvalidHealthImprovement)?
                .maximized()
            {
                u32::from(sides)
            } else {
                u32::from(die(&mut state, sides))
                    .saturating_add(u32::from(self.health_improvement))
                    .min(u32::from(sides))
            },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HdError {
    Arithmetic,
    TooManyHitDice,
    UnknownSource,
    InvalidHealthRoot,
    InvalidHealthImprovement,
}

/// Ordered advancement records. No caller can mutate a retained die's source
/// or health seed. A zero-HD ledger represents regression to persistent death;
/// actor state, rather than this definition, owns whether death has occurred.
///
/// Seeded ledgers retain their original ordinal Health stream for earned growth.
/// Recorded-only ledgers retain explicit per-die seeds without a known root;
/// they support manual advancement but cannot perform ordinary earned growth.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HdLedger {
    entries: Vec<HitDie>,
    health_root_seed: Option<u64>,
    drained: std::collections::BTreeMap<usize, HitDie>,
}

impl HdLedger {
    /// Assign independent health streams in advancement order. Class/type
    /// choices do not change the assigned stream; combat RNG is never borrowed.
    pub fn seeded(sources: Vec<HdSource>, seed: u64) -> Result<Self, HdError> {
        if sources.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        let mut state = seed;
        Ok(Self {
            entries: sources
                .into_iter()
                .enumerate()
                .map(|(ordinal, source)| {
                    HitDie::with_health_policy(
                        source,
                        random_word(&mut state),
                        HealthRollPolicy::for_ordinal(ordinal),
                    )
                })
                .collect(),
            health_root_seed: Some(seed),
            drained: Default::default(),
        })
    }

    /// Append from the actor's original health stream without changing retained
    /// records. Removing and re-adding an ordinal assigns the same health seed.
    /// Seeded ledgers reject a different root without mutation. Recorded-only
    /// ledgers accept explicit manual roots without claiming stream provenance.
    /// Seed lookup does constant work without replaying the root's prior words;
    /// the actual per-HD records remain authoritative.
    pub fn append_seeded(&mut self, source: HdSource, root_seed: u64) -> Result<(), HdError> {
        if self.entries.len() >= crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        if self.health_root_seed.is_some_and(|root| root != root_seed) {
            return Err(HdError::InvalidHealthRoot);
        }
        let health_seed = random_word_at(root_seed, HdCount::from_usize(self.entries.len())?.get());
        let restored = self
            .drained
            .remove(&self.entries.len())
            .filter(|entry| entry.source == source && entry.health_seed == health_seed);
        let ordinal = self.entries.len();
        self.entries.push(restored.unwrap_or_else(|| {
            HitDie::with_health_policy(source, health_seed, HealthRollPolicy::for_ordinal(ordinal))
        }));
        Ok(())
    }

    /// Construct a recorded-only ledger with explicit per-die Health seeds.
    /// Manual advancement remains available; ordinary earned growth requires
    /// an original Health stream and is unavailable for this ledger domain.
    pub fn new(entries: Vec<HitDie>) -> Result<Self, HdError> {
        Self::from_recorded(entries, None)
    }

    pub(crate) fn from_recorded(
        mut entries: Vec<HitDie>,
        health_root_seed: Option<u64>,
    ) -> Result<Self, HdError> {
        if entries.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        for (ordinal, entry) in entries.iter_mut().enumerate() {
            entry.bind_ordinal_policy(ordinal);
        }
        if let Some(mut stream) = health_root_seed {
            for (ordinal, entry) in entries.iter().enumerate() {
                if entry.health_seed != random_word(&mut stream)
                    || entry.health_policy != Some(HealthRollPolicy::for_ordinal(ordinal))
                {
                    return Err(HdError::InvalidHealthRoot);
                }
            }
        }
        Ok(Self {
            entries,
            health_root_seed,
            drained: Default::default(),
        })
    }

    /// Compose already-owned rows, preserving all acquisition provenance.
    /// The effective projection has no single ordinal stream; personal growth
    /// must append to its authoritative personal ledger, then recompose.
    pub fn composed(entries: Vec<HitDie>) -> Result<Self, HdError> {
        if entries.iter().any(|entry| entry.health_policy.is_none()) {
            return Err(HdError::InvalidHealthImprovement);
        }
        Self::from_recorded(entries, None)
    }

    pub(crate) fn restore_drained(&mut self, entries: Vec<(usize, HitDie)>) -> Result<(), HdError> {
        if entries.len() + self.entries.len() > crate::limits::HIT_DICE {
            return Err(HdError::TooManyHitDice);
        }
        let mut drained = std::collections::BTreeMap::new();
        for (index, (ordinal, mut entry)) in entries.into_iter().enumerate() {
            if ordinal != self.entries.len() + index
                || ordinal < self.entries.len()
                || ordinal >= crate::limits::HIT_DICE
                || drained.contains_key(&ordinal)
            {
                return Err(HdError::InvalidHealthImprovement);
            }
            entry.bind_ordinal_policy(ordinal);
            if self.health_root_seed.is_some_and(|root| {
                entry.health_seed != random_word_at(root, ordinal as u64)
                    || entry.health_policy != Some(HealthRollPolicy::for_ordinal(ordinal))
            }) {
                return Err(HdError::InvalidHealthRoot);
            }
            drained.insert(ordinal, entry);
        }
        self.drained = drained;
        Ok(())
    }
    pub(crate) fn drained(&self) -> impl Iterator<Item = (usize, &HitDie)> {
        self.drained.iter().map(|(ordinal, die)| (*ordinal, die))
    }
    pub(crate) fn bind_health_origins(
        &mut self,
        kind: &TypeId,
        templates: &[crate::content_ids::TemplateId],
        rules: &Ruleset,
    ) -> Result<(), HdError> {
        for entry in &mut self.entries {
            entry.bind_origin(kind, templates, rules)?;
        }
        for entry in self.drained.values_mut() {
            entry.bind_origin(kind, templates, rules)?;
        }
        Ok(())
    }
    pub(crate) fn improve_health(
        &mut self,
        claim: &HitDieHealthImprovement,
    ) -> Result<(), HdError> {
        let ordinal =
            usize::try_from(claim.ordinal.get()).map_err(|_| HdError::InvalidHealthImprovement)?;
        let entry = self
            .entries
            .get_mut(ordinal)
            .ok_or(HdError::InvalidHealthImprovement)?;
        if entry.improvement_claim(ordinal).as_ref() != Some(claim) {
            return Err(HdError::InvalidHealthImprovement);
        }
        entry.health_improvement += 1;
        Ok(())
    }
    /// Root of the ordinal Health stream, retained through drain and regrowth.
    /// `None` identifies a recorded-only ledger, ineligible for earned growth.
    pub fn health_root_seed(&self) -> Option<u64> {
        self.health_root_seed
    }

    pub fn entries(&self) -> &[HitDie] {
        &self.entries
    }

    pub fn total_hd(&self) -> HdCount {
        HdCount::from_usize(self.entries.len()).expect("admitted resident ledger count")
    }

    pub fn racial_hd(&self) -> HdCount {
        HdCount::from_usize(
            self.entries
                .iter()
                .filter(|entry| {
                    crate::diagnostics::raw_hit_die_visit();
                    entry.source == HdSource::Racial
                })
                .count(),
        )
        .expect("admitted resident ledger count")
    }

    pub fn class_level(&self, class: &ClassId) -> HdCount {
        HdCount::from_usize(
            self.entries
                .iter()
                .filter(|entry| {
                    crate::diagnostics::raw_hit_die_visit();
                    matches!(&entry.source, HdSource::Class(id) if id == class)
                })
                .count(),
        )
        .expect("admitted resident ledger count")
    }

    /// Checked aggregate grants across independent pools and actual HD sources.
    pub fn advancement_points(&self, rules: &Ruleset) -> Result<u64, HdError> {
        self.entries
            .iter()
            .map(|entry| {
                crate::diagnostics::raw_hit_die_visit();
                rules
                    .point_grants(&entry.source)
                    .map(|grants| {
                        grants
                            .iter()
                            .try_fold(0_u64, |sum, grant| sum.checked_add(u64::from(grant.points)))
                            .ok_or(HdError::Arithmetic)
                    })
                    .ok_or(HdError::UnknownSource)?
            })
            .try_fold(0_u64, |sum, points| {
                sum.checked_add(points?).ok_or(HdError::Arithmetic)
            })
    }

    pub fn attribute_opportunities(&self, rules: &Ruleset) -> HdCount {
        HdCount(self.total_hd().get() / u64::from(rules.constants().attribute_opportunity_interval))
    }
    pub fn health_provenance(
        &self,
        kind: &TypeId,
        rules: &Ruleset,
    ) -> Result<Vec<HealthContribution>, HdError> {
        self.health_provenance_with_improvement(kind, rules)
            .map(|(health, _)| health)
    }
    pub(crate) fn health_provenance_with_improvement(
        &self,
        kind: &TypeId,
        rules: &Ruleset,
    ) -> Result<(Vec<HealthContribution>, Vec<HitDieHealthImprovement>), HdError> {
        let mut next = Vec::with_capacity(2);
        let health = self
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                if next.len() < 2 {
                    if let Some(mut claim) = entry.improvement_claim(index) {
                        next.push(claim.clone());
                        claim.previous += 1;
                        if next.len() < 2 && claim.valid() {
                            next.push(claim);
                        }
                    }
                }
                entry.health(kind, rules)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((health, next))
    }
    pub fn remove_latest(&mut self) -> Option<HitDie> {
        let removed = self.entries.pop()?;
        self.drained.insert(self.entries.len(), removed.clone());
        Some(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_append_matches_every_owned_ordinal_without_revisiting_the_root_prefix() {
        for root in [0, u64::MAX, 0x0123_4567_89ab_cdef] {
            let expected =
                HdLedger::seeded(vec![HdSource::Racial; crate::limits::HIT_DICE], root).unwrap();
            let mut ledger = HdLedger::seeded(vec![], root).unwrap();
            for ordinal in 0..crate::limits::HIT_DICE {
                let draws_before = crate::dice::random_word_draws_for_test();
                ledger.append_seeded(HdSource::Racial, root).unwrap();
                let draws = crate::dice::random_word_draws_for_test() - draws_before;
                assert_eq!(ledger.entries(), &expected.entries()[..=ordinal]);
                assert_eq!(draws, 1, "root {root:#x}, zero-based ordinal {ordinal}");
            }
            assert_eq!(ledger, expected);
        }
    }

    #[test]
    fn recorded_health_roots_validate_every_owned_ordinal() {
        let ledger = HdLedger::seeded(vec![HdSource::Racial; 3], 42).unwrap();
        assert_eq!(
            HdLedger::from_recorded(ledger.entries().to_vec(), Some(42)),
            Ok(ledger.clone())
        );
        for ordinal in 0..3 {
            let mut entries = ledger.entries().to_vec();
            entries[ordinal].health_seed ^= 1;
            assert_eq!(
                HdLedger::from_recorded(entries, Some(42)),
                Err(HdError::InvalidHealthRoot)
            );
        }
    }

    #[test]
    fn empty_seeded_ledgers_preserve_zero_and_maximum_roots() {
        for root in [0, u64::MAX] {
            let mut ledger = HdLedger::seeded(vec![], root).unwrap();
            assert_eq!(ledger.health_root_seed(), Some(root));
            assert_eq!(
                HdLedger::from_recorded(vec![], Some(root)),
                Ok(ledger.clone())
            );
            ledger.append_seeded(HdSource::Racial, root).unwrap();
            assert_eq!(
                ledger,
                HdLedger::seeded(vec![HdSource::Racial], root).unwrap()
            );
        }
    }

    #[test]
    fn manual_ledgers_do_not_infer_a_root_from_later_seeded_appends() {
        for entries in [vec![], vec![HitDie::new(HdSource::Racial, 42)]] {
            let mut ledger = HdLedger::new(entries).unwrap();
            ledger.append_seeded(HdSource::Racial, 42).unwrap();
            assert_eq!(ledger.health_root_seed(), None);
            assert_eq!(
                HdLedger::from_recorded(ledger.entries().to_vec(), None),
                Ok(ledger)
            );
        }
    }

    #[test]
    fn composition_keeps_personal_roll_when_package_size_or_position_changes() {
        let rules = crate::test_support::test_ruleset();
        let kind = TypeId::new("humanoid").unwrap();
        let seed = (0..100)
            .find(|seed| original_roll(*seed, 8, false) < 7)
            .unwrap();
        let person = HitDie::with_health_policy(HdSource::Racial, seed, HealthRollPolicy::Seeded);
        let body = HitDie::with_health_policy(HdSource::Racial, 23, HealthRollPolicy::Maximized);
        let mut before = HdLedger::composed(vec![body.clone(), person.clone()]).unwrap();
        before.bind_health_origins(&kind, &[], &rules).unwrap();
        let original = before.health_provenance(&kind, &rules).unwrap();
        assert_eq!(original[0].health, 8);
        assert!(original[1].health < 7);
        let personal = before.entries()[1].clone();
        let mut alone = HdLedger::composed(vec![personal.clone()]).unwrap();
        alone.bind_health_origins(&kind, &[], &rules).unwrap();
        assert_eq!(
            alone.health_provenance(&kind, &rules).unwrap()[0],
            original[1]
        );
        let extra = HitDie::with_health_policy(HdSource::Racial, 41, HealthRollPolicy::Seeded);
        let mut expanded = HdLedger::composed(vec![body, extra, personal.clone()]).unwrap();
        expanded.bind_health_origins(&kind, &[], &rules).unwrap();
        assert_eq!(expanded.entries()[2], personal);
        assert_eq!(
            expanded.health_provenance(&kind, &rules).unwrap()[2],
            original[1]
        );
        assert_eq!(expanded.health_root_seed(), None);
    }

    #[test]
    fn composed_health_improvement_and_origin_survive_reordering() {
        let rules = crate::test_support::test_ruleset();
        let kind = TypeId::new("humanoid").unwrap();
        let seed = (0..100)
            .find(|seed| original_roll(*seed, 8, false) < 7)
            .unwrap();
        let mut ledger = HdLedger::composed(vec![
            HitDie::with_health_policy(HdSource::Racial, 19, HealthRollPolicy::Maximized),
            HitDie::with_health_policy(HdSource::Racial, seed, HealthRollPolicy::Seeded),
        ])
        .unwrap();
        ledger.bind_health_origins(&kind, &[], &rules).unwrap();
        let (_, claims) = ledger
            .health_provenance_with_improvement(&kind, &rules)
            .unwrap();
        let old_claim = claims[0].clone();
        ledger.improve_health(&old_claim).unwrap();
        let personal = ledger.entries()[1].clone();
        let before = ledger.health_provenance(&kind, &rules).unwrap()[1].clone();
        let mut composed = HdLedger::composed(vec![personal.clone()]).unwrap();
        assert_eq!(composed.entries()[0].health_improvement(), 1);
        assert_eq!(
            composed.entries()[0].health_origin(),
            personal.health_origin()
        );
        assert_eq!(
            composed.health_provenance(&kind, &rules).unwrap()[0],
            before
        );
        assert_eq!(
            composed.improve_health(&old_claim),
            Err(HdError::InvalidHealthImprovement)
        );
        let (_, claims) = composed
            .health_provenance_with_improvement(&kind, &rules)
            .unwrap();
        assert_eq!(claims[0].ordinal.get(), 0);
        assert_eq!(claims[0].health_policy, HealthRollPolicy::Seeded);
        assert_eq!(claims[0].previous, 1);
        composed.improve_health(&claims[0]).unwrap();
        assert_eq!(composed.entries()[0].health_improvement(), 2);
        assert_eq!(
            personal.with_health_roll_policy(HealthRollPolicy::Maximized),
            Err(HdError::InvalidHealthImprovement)
        );
    }

    #[test]
    fn composition_rejects_unbound_rolls_and_root_provenance_rejects_policy_forgery() {
        assert_eq!(
            HdLedger::composed(vec![HitDie::new(HdSource::Racial, 7)]),
            Err(HdError::InvalidHealthImprovement)
        );
        let ledger = HdLedger::seeded(vec![HdSource::Racial; 2], 42).unwrap();
        let mut forged = ledger.entries().to_vec();
        forged[1].health_policy = Some(HealthRollPolicy::Maximized);
        assert_eq!(
            HdLedger::from_recorded(forged, Some(42)),
            Err(HdError::InvalidHealthRoot)
        );
        let mut ledger = ledger;
        let removed = ledger.remove_latest().unwrap();
        ledger.append_seeded(HdSource::Racial, 42).unwrap();
        assert_eq!(ledger.entries()[1], removed);
    }

    #[test]
    fn stable_owner_cannot_be_reassigned_or_have_its_roll_policy_changed() {
        use crate::creatures::composition::{HdOwner, PersonalHdId};
        let owner = HdOwner::Personal {
            id: PersonalHdId(std::num::NonZeroU64::new(9).unwrap()),
        };
        let row = HitDie::new(HdSource::Racial, 31)
            .with_owner(owner.clone())
            .unwrap();
        assert_eq!(row.owner(), Some(&owner));
        assert_eq!(row.health_policy(), Some(HealthRollPolicy::Seeded));
        assert_eq!(row.clone().with_owner(owner.clone()), Ok(row.clone()));
        assert_eq!(
            row.clone()
                .with_health_roll_policy(HealthRollPolicy::Maximized),
            Err(HdError::InvalidHealthImprovement)
        );
        let other = HdOwner::Personal {
            id: PersonalHdId(std::num::NonZeroU64::new(10).unwrap()),
        };
        assert_eq!(
            row.with_owner(other),
            Err(HdError::InvalidHealthImprovement)
        );
    }
}
