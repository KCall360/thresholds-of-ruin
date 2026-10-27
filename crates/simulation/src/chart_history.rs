//! Actor-owned historical Food/Magic/Mapping samples and separate latest attempt facts.
mod codec;
use crate::{chart, scope_projection, spatial_scope::registry::Registry, ActorId};
pub(crate) use codec::{local_owners, Pool as StoredPool, RestoreCache};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tor_world::{Location, Shared, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Food,
    Magic,
    Mapping,
}
impl Kind {
    fn chart_kind(self) -> chart::Kind {
        match self {
            Self::Food => chart::Kind::Food,
            Self::Magic => chart::Kind::Magic,
            Self::Mapping => chart::Kind::Mapping,
        }
    }
}
/// Existing authoritative immutable roots, not a second transform graph.
/// Save encodes their existing World/spatial pool indices, never nested copies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquisitionAuthority {
    pub world: Shared<World>,
    pub registry: Shared<Registry>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoricalChart {
    kind: Kind,
    acquired_tick: u64,
    scope: String,
    authority: Shared<AcquisitionAuthority>,
    chart: Arc<chart::Chart>,
    /// One point per row, in exactly the row occurrence order. Private proof only.
    source: Arc<SourceProof>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum SourceProof {
    Points(Vec<Location>),
    Mapping(Vec<crate::mapping_chart::SourceRow>),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(
    tag = "kind",
    content = "reason",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum AttemptStatus {
    Empty,
    Positive,
    Unavailable(UnavailableReason),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    Budget,
    Projection,
}
impl From<chart::Unavailable> for UnavailableReason {
    fn from(reason: chart::Unavailable) -> Self {
        match reason {
            chart::Unavailable::Budget => Self::Budget,
            chart::Unavailable::Projection => Self::Projection,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub attempted_at: u64,
    pub status: AttemptStatus,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot {
    latest: Option<Shared<HistoricalChart>>,
    attempt: Option<Attempt>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Owner {
    food: Slot,
    magic: Slot,
    mapping: Slot,
}
impl Owner {
    pub fn get(&self, kind: Kind) -> &Slot {
        match kind {
            Kind::Food => &self.food,
            Kind::Magic => &self.magic,
            Kind::Mapping => &self.mapping,
        }
    }
    pub fn empty(&self) -> bool {
        self.food.attempt.is_none()
            && self.magic.attempt.is_none()
            && self.mapping.attempt.is_none()
    }
    /// Final candidate already passed whole-delivery admission. Positive carries
    /// exactly its new historical root; other statuses never replace the old root.
    /// No learning authority is retained. Validate everything before mutation.
    pub fn record_attempt(
        &mut self,
        kind: Kind,
        attempt: Attempt,
        replacement: Option<Shared<HistoricalChart>>,
    ) -> Result<bool, Error> {
        let slot = match kind {
            Kind::Food => &mut self.food,
            Kind::Magic => &mut self.magic,
            Kind::Mapping => &mut self.mapping,
        };
        if slot
            .attempt
            .is_some_and(|old| old.attempted_at > attempt.attempted_at)
        {
            return Err(Error::Structure);
        }
        match (attempt.status, replacement.as_ref()) {
            (AttemptStatus::Positive, Some(root))
                if root.kind == kind && root.acquired_tick == attempt.attempted_at => {}
            (AttemptStatus::Empty | AttemptStatus::Unavailable(_), None) => {}
            _ => return Err(Error::Structure),
        }
        let changed = slot.attempt != Some(attempt)
            || replacement
                .as_ref()
                .is_some_and(|root| slot.latest.as_ref() != Some(root));
        if replacement.is_some() {
            slot.latest = replacement;
        }
        slot.attempt = Some(attempt);
        Ok(changed)
    }
}
impl Slot {
    pub fn latest(&self) -> Option<&Shared<HistoricalChart>> {
        self.latest.as_ref()
    }
    pub fn attempt(&self) -> Option<Attempt> {
        self.attempt
    }
}
#[derive(Clone, Copy)]
pub struct ValidationBudget {
    pub scope_bytes: usize,
    pub chart: chart::Budget,
    pub projection: scope_projection::Budget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Structure,
    Reference,
    Source,
    Projection,
    Capacity,
}
impl HistoricalChart {
    /// Explicit acquisition witness. Does not itself authorize publication or ID.
    pub fn checked(
        kind: Kind,
        acquired_tick: u64,
        scope: String,
        authority: Shared<AcquisitionAuthority>,
        chart: Arc<chart::Chart>,
        source_points: Vec<Location>,
        budget: ValidationBudget,
    ) -> Result<Self, Error> {
        if !authority.world.historical_checkpoint_valid() {
            return Err(Error::Source);
        }
        Self::checked_after_world_validation(
            kind,
            acquired_tick,
            scope,
            authority,
            chart,
            source_points,
            budget,
        )
    }
    /// Caller must have proved the exact immutable World structure, cached by
    /// its pool index. Projection still validates this payload independently.
    fn checked_after_world_validation(
        kind: Kind,
        acquired_tick: u64,
        scope: String,
        authority: Shared<AcquisitionAuthority>,
        chart: Arc<chart::Chart>,
        source_points: Vec<Location>,
        budget: ValidationBudget,
    ) -> Result<Self, Error> {
        let value = Self {
            kind,
            acquired_tick,
            scope,
            authority,
            chart,
            source: Arc::new(SourceProof::Points(source_points)),
        };
        value.validate_geometry(budget)?;
        Ok(value)
    }
    pub(crate) fn checked_mapping(
        acquired_tick: u64,
        scope: String,
        authority: Shared<AcquisitionAuthority>,
        chart: Arc<chart::Chart>,
        rows: Vec<crate::mapping_chart::SourceRow>,
        budget: ValidationBudget,
    ) -> Result<Self, Error> {
        if !authority.world.historical_checkpoint_valid() {
            return Err(Error::Source);
        }
        Self::mapping_after_world_validation(acquired_tick, scope, authority, chart, rows, budget)
    }
    fn mapping_after_world_validation(
        acquired_tick: u64,
        scope: String,
        authority: Shared<AcquisitionAuthority>,
        chart: Arc<chart::Chart>,
        rows: Vec<crate::mapping_chart::SourceRow>,
        budget: ValidationBudget,
    ) -> Result<Self, Error> {
        let value = Self {
            kind: Kind::Mapping,
            acquired_tick,
            scope,
            authority,
            chart,
            source: Arc::new(SourceProof::Mapping(rows)),
        };
        value.validate_geometry(budget)?;
        Ok(value)
    }
    pub fn kind(&self) -> Kind {
        self.kind
    }
    pub fn acquired_tick(&self) -> u64 {
        self.acquired_tick
    }
    /// Adapter must always present this as historical, never current detection.
    pub fn shared_chart(&self) -> &Arc<chart::Chart> {
        &self.chart
    }
    pub fn chart(&self) -> &chart::Chart {
        &self.chart
    }
    fn validate_geometry(&self, budget: ValidationBudget) -> Result<(), Error> {
        if self.scope.is_empty()
            || self.scope.chars().any(char::is_control)
            || self.chart.kind() != self.kind.chart_kind()
            || self.chart.rows().is_empty()
        {
            return Err(Error::Structure);
        }
        if self.scope.len() > budget.scope_bytes {
            return Err(Error::Capacity);
        }
        // Chart is immutable and constructor-validated. The future strict DTO
        // decoder must invoke that constructor; no redundant Vec clone here.
        if self.chart.components().len() > budget.chart.components
            || self.chart.rows().len() > budget.chart.rows
            || self.chart.encoded_size_bound() > budget.chart.json_bytes
        {
            return Err(Error::Capacity);
        }
        // Source witnesses retain the producer's exact row-occurrence order;
        // equal public rows never authorize sorting or deduplicating samples.
        let scope = self
            .authority
            .registry
            .scopes()
            .find(|s| s.id == self.scope)
            .ok_or(Error::Source)?;
        let points = match self.source.as_ref() {
            SourceProof::Mapping(rows) if self.kind == Kind::Mapping => {
                return crate::mapping_history::validate_mapping_history(
                    &self.authority.world,
                    scope,
                    &self.chart,
                    rows,
                    budget,
                );
            }
            SourceProof::Points(points) if self.kind != Kind::Mapping => points,
            _ => return Err(Error::Structure),
        };
        if points.len() != self.chart.rows().len() {
            return Err(Error::Structure);
        }
        for point in points {
            if scope.members.binary_search(&point.region).is_err()
                || !self.authority.world.contains(*point)
            {
                return Err(Error::Source);
            }
        }
        let projection = scope_projection::project(
            &self.authority.world,
            &scope.members,
            points,
            budget.projection,
        )
        .map_err(|error| match error {
            scope_projection::Error::Budget
            | scope_projection::Error::Allocation
            | scope_projection::Error::Arithmetic => Error::Capacity,
            scope_projection::Error::InvalidSource => Error::Source,
        })?;
        if projection.components.len() != self.chart.components().len()
            || projection
                .components
                .iter()
                .zip(self.chart.components())
                .any(|(a, b)| a.alignment != b.alignment)
            || projection.samples.len() != self.chart.rows().len()
            || projection
                .samples
                .iter()
                .zip(self.chart.rows())
                .any(|(a, b)| a.component != b.component || a.position != b.position)
        {
            return Err(Error::Projection);
        }
        Ok(())
    }
}

/// Conservative archive envelopes derived from the existing raw archive ceiling,
/// not sight/population limits. Projection budgets remain independently explicit.
pub fn archive_budget() -> ValidationBudget {
    let bytes = crate::checkpoint::ARCHIVE_BYTE_LIMIT;
    ValidationBudget {
        scope_bytes: bytes / 6,
        chart: chart::Budget {
            components: bytes / 32,
            rows: bytes / 128,
            json_bytes: bytes,
        },
        projection: scope_projection::Budget {
            work: bytes,
            allocation_bytes: bytes,
            regions: bytes / 32,
            directed_constraints: bytes / 32,
            samples: bytes / 128,
        },
    }
}
impl crate::Game {
    pub fn chart_history(&self, actor: ActorId) -> Option<&Owner> {
        self.chart_history.get(&actor)
    }
    pub(crate) fn capture_historical_chart(
        &self,
        kind: Kind,
        scope: String,
        source_points: Vec<Location>,
        chart: Arc<chart::Chart>,
        budget: ValidationBudget,
    ) -> Result<Shared<HistoricalChart>, Error> {
        HistoricalChart::checked(
            kind,
            self.tick,
            scope,
            Shared::new(AcquisitionAuthority {
                world: self.world.clone(),
                registry: self.spatial_registry.clone(),
            }),
            chart,
            source_points,
            budget,
        )
        .map(Shared::new)
    }
    /// Source-audit-only immutable authority; never a client projection.
    pub fn historical_chart_authorities(
        &self,
    ) -> impl Iterator<Item = (&Shared<World>, &Shared<Registry>, &str)> {
        authority_sources(&self.chart_history)
    }
    pub fn historical_chart_sources(&self) -> impl Iterator<Item = (&World, &Registry, &str)> {
        sources(&self.chart_history)
    }
}
pub(crate) fn authority_sources(
    owners: &BTreeMap<ActorId, Owner>,
) -> impl Iterator<Item = (&Shared<World>, &Shared<Registry>, &str)> {
    owners
        .values()
        .flat_map(|o| [&o.food, &o.magic, &o.mapping])
        .filter_map(|slot| slot.latest.as_ref())
        .map(|root| {
            (
                &root.authority.world,
                &root.authority.registry,
                root.scope.as_str(),
            )
        })
}
pub(crate) fn sources(
    owners: &BTreeMap<ActorId, Owner>,
) -> impl Iterator<Item = (&World, &Registry, &str)> {
    owners
        .values()
        .flat_map(|o| [&o.food, &o.magic, &o.mapping])
        .filter_map(|slot| slot.latest.as_ref())
        .map(|root| {
            (
                &*root.authority.world,
                &*root.authority.registry,
                root.scope.as_str(),
            )
        })
}
pub(crate) fn owners_valid(
    owners: &BTreeMap<ActorId, Owner>,
    tick: u64,
    mut actor_exists: impl FnMut(ActorId) -> bool,
) -> bool {
    owners.iter().all(|(id, owner)| {
        id.0 != 0
            && actor_exists(*id)
            && !owner.empty()
            && [
                (Kind::Food, &owner.food),
                (Kind::Magic, &owner.magic),
                (Kind::Mapping, &owner.mapping),
            ]
            .into_iter()
            .all(|(kind, slot)| match (slot.attempt, slot.latest.as_ref()) {
                (None, None) => true,
                (Some(attempt), root) if attempt.attempted_at <= tick => match root {
                    None => !matches!(attempt.status, AttemptStatus::Positive),
                    Some(root) => {
                        root.kind == kind
                            && root.acquired_tick <= attempt.attempted_at
                            && (!matches!(attempt.status, AttemptStatus::Positive)
                                || root.acquired_tick == attempt.attempted_at)
                    }
                },
                _ => false,
            })
    })
}
#[cfg(test)]
pub(crate) mod tests;

impl<'de> Deserialize<'de> for Attempt {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            attempted_at: u64,
            status: AttemptStatus,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            attempted_at: f.attempted_at,
            status: f.status,
        })
    }
}

impl<'de> Deserialize<'de> for AttemptStatus {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            content = "reason",
            rename_all = "snake_case",
            deny_unknown_fields
        )]
        enum Fields {
            Empty,
            Positive,
            Unavailable(UnavailableReason),
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Empty => Self::Empty,
            Fields::Positive => Self::Positive,
            Fields::Unavailable(reason) => Self::Unavailable(reason),
        })
    }
}

// Companion within chart_history.rs impl Game. Pointer equality is the fast
// unchanged path, NOT sole authority: equal-value COW copies also bypass.
impl crate::Game {
    pub fn chart_history_changed(&self, previous: &Self) -> bool {
        !self.chart_history.shares_storage(&previous.chart_history)
            && self.chart_history != previous.chart_history
    }
}

#[cfg(test)]
#[path = "chart_history/mapping_tests.rs"]
mod mapping_tests;
