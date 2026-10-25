//! Pure whole-floor census. No query learning, action handles, or resident-only fallback.
use crate::appearance_catalog::CreatureAppearance;
use crate::appearance_state::{AppearanceChannel, AppearanceKey};
use crate::chart::{self, Candidate, Component, Kind, Mark, Row, Unavailable};
use crate::{ActorId, Game, GameError, ItemClass, ItemLocation};
use std::sync::Arc;
use tor_world::Location;

/// Presentation capacity only: does not narrow source or spatial admission.
/// Independently measured maximal generated/authored positives own this bound.
pub const CANONICAL_DELIVERY_BYTES: usize = crate::checkpoint::ARCHIVE_BYTE_LIMIT;
#[derive(Clone, Copy)]
pub(crate) struct CensusBudget {
    pub work: usize,
    pub allocation_bytes: usize,
    pub delivery_bytes: usize,
    pub pages: chart::delivery::PageLimits,
}
impl CensusBudget {
    pub(crate) fn canonical() -> Self {
        Self {
            work: crate::checkpoint::ARCHIVE_BYTE_LIMIT,
            allocation_bytes: crate::checkpoint::ARCHIVE_BYTE_LIMIT,
            delivery_bytes: CANONICAL_DELIVERY_BYTES,
            pages: chart::delivery::PageLimits::canonical(),
        }
    }
}
/// Source positions are private historical proof, never wire/action references.
pub(crate) struct PreparedDetection {
    pub candidate: Candidate,
    pub scope: Option<String>,
    pub source_points: Vec<Location>,
}
impl PreparedDetection {
    fn unavailable(reason: Unavailable) -> Self {
        Self {
            candidate: Candidate::unavailable(reason),
            scope: None,
            source_points: vec![],
        }
    }
}
#[derive(Clone)]
struct Sample {
    at: Location,
    mark: Mark,
}
fn pay(left: &mut usize, amount: usize) -> Result<(), Unavailable> {
    *left = left.checked_sub(amount).ok_or(Unavailable::Budget)?;
    Ok(())
}
fn reserve<T>(count: usize, budget: &mut CensusBudget) -> Result<Vec<T>, Unavailable> {
    pay(&mut budget.work, count)?;
    pay(
        &mut budget.allocation_bytes,
        count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(Unavailable::Budget)?,
    )?;
    let mut result = Vec::new();
    result
        .try_reserve_exact(count)
        .map_err(|_| Unavailable::Budget)?;
    Ok(result)
}
fn sorting(count: usize, budget: &mut CensusBudget) -> Result<(), Unavailable> {
    let logarithm = usize::BITS as usize - count.leading_zeros() as usize + 1;
    pay(
        &mut budget.work,
        count
            .checked_mul(logarithm)
            .and_then(|v| v.checked_mul(16))
            .ok_or(Unavailable::Budget)?,
    )
}
impl Game {
    /// Caller has loaded the complete canonical scope through ordinary load-only
    /// transitions. Missing members yield Unavailable, never a successful prefix.
    pub(crate) fn prepare_detection_chart(
        &self,
        observer: ActorId,
        kind: Kind,
        budget: CensusBudget,
    ) -> Result<PreparedDetection, GameError> {
        if kind == Kind::Mapping {
            return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan));
        }
        let owner = self.actors.get(&observer).ok_or(GameError::UnknownActor)?;
        let hallucination = if kind == Kind::Monsters {
            self.projected_hallucination(observer)?
        } else {
            None
        };
        let build = || -> Result<PreparedDetection, Unavailable> {
            let mut budget = budget;
            let mut scope = None;
            for candidate in self.spatial_registry.scopes() {
                pay(
                    &mut budget.work,
                    1 + usize::BITS as usize - candidate.members.len().leading_zeros() as usize,
                )?;
                if candidate
                    .members
                    .binary_search(&owner.location.region)
                    .is_ok()
                {
                    scope = Some(candidate);
                    break;
                }
            }
            let scope = scope.ok_or(Unavailable::Projection)?;
            for region in &scope.members {
                pay(&mut budget.work, 1)?;
                if self.world.region(*region).is_none() {
                    return Err(Unavailable::Projection);
                }
            }
            let contains = |at: Location, budget: &mut CensusBudget| -> Result<bool, Unavailable> {
                pay(
                    &mut budget.work,
                    1 + usize::BITS as usize - scope.members.len().leading_zeros() as usize,
                )?;
                Ok(scope.members.binary_search(&at.region).is_ok())
            };
            // Two passes charge complete population traversal before allocation.
            let mut count = 0usize;
            if kind == Kind::Monsters {
                for (id, actor) in self.actors.iter() {
                    pay(&mut budget.work, 1)?;
                    if *id != observer
                        && actor.combat.is_some()
                        && actor.alive()
                        && contains(actor.location, &mut budget)?
                    {
                        count = count.checked_add(1).ok_or(Unavailable::Budget)?;
                    }
                }
            } else {
                for item in self.items.values() {
                    pay(&mut budget.work, 1)?;
                    if let Some(at) = self.detection_item_location(observer, kind, item) {
                        if contains(at, &mut budget)? {
                            count = count.checked_add(1).ok_or(Unavailable::Budget)?;
                        }
                    }
                }
            }
            let mut samples = reserve::<Sample>(count, &mut budget)?;
            // One shared neutral tuple for unnamed species, not personal identity.
            pay(
                &mut budget.allocation_bytes,
                2 * std::mem::size_of::<usize>(),
            )?;
            let neutral = CreatureAppearance {
                name: Arc::from(""),
                asset: None,
            };
            if kind == Kind::Monsters {
                for (id, actor) in self.actors.iter() {
                    pay(&mut budget.work, 1)?;
                    let Some(combat) = actor.combat.as_ref() else {
                        continue;
                    };
                    if *id == observer || !actor.alive() || !contains(actor.location, &mut budget)?
                    {
                        continue;
                    }
                    let appearance = if let Some(clock) = &hallucination {
                        let value = self.content.presentation_catalog().creature(AppearanceKey {
                            seed: self.seed,
                            observer: observer.0,
                            subject: id.0,
                            channel: AppearanceChannel::MonsterSense,
                            phase: clock.phase(),
                        });
                        CreatureAppearance {
                            name: value.name,
                            asset: value.asset,
                        }
                    } else {
                        self.content
                            .detection_appearance(combat.creature().build().species_id())
                            .unwrap_or(&neutral)
                            .clone()
                    };
                    samples.push(Sample {
                        at: actor.location,
                        mark: Mark::Monster(appearance),
                    });
                }
            } else {
                for item in self.items.values() {
                    pay(&mut budget.work, 1)?;
                    let Some(at) = self.detection_item_location(observer, kind, item) else {
                        continue;
                    };
                    if contains(at, &mut budget)? {
                        samples.push(Sample {
                            at,
                            mark: if kind == Kind::Food {
                                Mark::Food
                            } else {
                                Mark::Magic
                            },
                        });
                    }
                }
                // Presence at a true location, not carried-item/stack quantities.
                // Never deduplicate after projection: distinct places may overlap.
                sorting(samples.len(), &mut budget)?;
                samples.sort_unstable_by_key(|sample| sample.at);
                pay(&mut budget.work, samples.len())?;
                samples.dedup_by_key(|sample| sample.at);
            }
            let mut points = reserve::<Location>(samples.len(), &mut budget)?;
            points.extend(samples.iter().map(|sample| sample.at));
            let projection = crate::scope_projection::project(
                &self.world,
                &scope.members,
                &points,
                crate::scope_projection::Budget {
                    work: budget.work,
                    allocation_bytes: budget.allocation_bytes,
                    regions: scope.members.len(),
                    directed_constraints: usize::MAX,
                    samples: points.len(),
                },
            )
            .map_err(|error| match error {
                crate::scope_projection::Error::InvalidSource => Unavailable::Projection,
                _ => Unavailable::Budget,
            })?;
            pay(&mut budget.work, projection.usage.work)?;
            pay(
                &mut budget.allocation_bytes,
                projection.usage.allocation_bytes,
            )?;
            let mut ordered = reserve::<(Row, Location)>(samples.len(), &mut budget)?;
            for (sample, projected) in samples.into_iter().zip(projection.samples) {
                ordered.push((
                    Row {
                        component: projected.component,
                        position: projected.position,
                        mark: sample.mark,
                    },
                    sample.at,
                ));
            }
            sorting(ordered.len(), &mut budget)?;
            ordered.sort_unstable();
            let mut rows = reserve::<Row>(ordered.len(), &mut budget)?;
            let mut source_points = reserve::<Location>(ordered.len(), &mut budget)?;
            for (row, at) in ordered {
                rows.push(row);
                source_points.push(at);
            }
            let mut components = reserve::<Component>(projection.components.len(), &mut budget)?;
            components.extend(projection.components.into_iter().map(|c| Component {
                alignment: c.alignment,
            }));
            // Grammar and size walks are bounded by source label limits. This is
            // conservative work charging, not an elapsed-time performance claim.
            pay(
                &mut budget.work,
                rows.len().checked_mul(1024).ok_or(Unavailable::Budget)?,
            )?;
            pay(
                &mut budget.allocation_bytes,
                std::mem::size_of::<chart::Chart>() + 2 * std::mem::size_of::<usize>(),
            )?;
            let candidate = Candidate::from_complete(
                kind,
                components,
                rows,
                chart::Budget {
                    components: scope.members.len(),
                    rows: source_points.len(),
                    json_bytes: budget.delivery_bytes,
                },
            )
            .map_err(|_| Unavailable::Projection)?;
            let Some(chart) = candidate.chart() else {
                return Err(candidate
                    .unavailable_reason()
                    .unwrap_or(Unavailable::Budget));
            };
            let summary = chart
                .delivery_summary(budget.pages)
                .map_err(|_| Unavailable::Budget)?;
            if summary.encoded_bytes_bound > budget.delivery_bytes {
                return Err(Unavailable::Budget);
            }
            pay(&mut budget.allocation_bytes, scope.id.len())?;
            Ok(PreparedDetection {
                candidate,
                scope: Some(scope.id.clone()),
                source_points,
            })
        };
        Ok(build().unwrap_or_else(PreparedDetection::unavailable))
    }
    fn detection_item_location(
        &self,
        observer: ActorId,
        kind: Kind,
        item: &crate::Item,
    ) -> Option<Location> {
        match (kind, item.location) {
            (Kind::Food, ItemLocation::Ground(at)) if item.spec.class == ItemClass::Food => {
                Some(at)
            }
            (Kind::Magic, ItemLocation::Ground(at)) if item.detection_magic() => Some(at),
            (Kind::Magic, ItemLocation::Carried(carrier))
                if carrier != observer && item.detection_magic() =>
            {
                let actor = self.actors.get(&carrier)?;
                actor.combat.as_ref().map(|_| actor.location)
            }
            _ => None,
        }
    }
}

/// Backend-only prepared view. The host supplies opaque tokens/correlation and
/// overrides historical=true when explicitly presenting a retained boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectionView {
    /// Current preparation support, independent of retained historical knowledge.
    pub supported: bool,
    pub kind: Kind,
    pub historical: bool,
    pub acquired_at: Option<u64>,
    pub candidate: Option<Candidate>,
    pub latest_attempt: Option<crate::chart_history::Attempt>,
}
/// Ephemeral final completion, never persisted as a pending learning flag.
pub(crate) struct ChartCompletion {
    pub(crate) candidate: Candidate,
    pub(crate) history_changed: bool,
}
impl ChartCompletion {
    pub(crate) fn admitted_nonempty(&self) -> bool {
        self.candidate
            .chart()
            .is_some_and(|chart| !chart.rows().is_empty())
    }
}
impl Game {
    pub fn detection_chart(&self, actor: ActorId, kind: Kind) -> Result<DetectionView, GameError> {
        if kind == Kind::Mapping {
            return self.mapping_chart(actor);
        }
        if !self.actors.contains_key(&actor) {
            return Err(GameError::UnknownActor);
        }
        if kind == Kind::Monsters {
            let active = self.monster_detection_active(actor);
            return Ok(DetectionView {
                supported: true,
                kind,
                historical: false,
                acquired_at: active.then_some(self.tick),
                candidate: if active {
                    Some(
                        self.prepare_detection_chart(actor, kind, CensusBudget::canonical())?
                            .candidate,
                    )
                } else {
                    None
                },
                latest_attempt: None,
            });
        }
        let history_kind = if kind == Kind::Food {
            crate::chart_history::Kind::Food
        } else {
            crate::chart_history::Kind::Magic
        };
        let slot = self
            .chart_history
            .get(&actor)
            .map(|owner| owner.get(history_kind));
        let latest = slot.and_then(|slot| slot.latest());
        Ok(DetectionView {
            supported: true,
            kind,
            historical: true,
            acquired_at: latest.map(|root| root.acquired_tick()),
            candidate: latest.map(|root| Candidate::from_shared_chart(root.shared_chart().clone())),
            latest_attempt: slot.and_then(|slot| slot.attempt()),
        })
    }
    pub(crate) fn complete_detection_chart(
        &mut self,
        actor: ActorId,
        kind: Kind,
    ) -> Result<ChartCompletion, GameError> {
        self.complete_detection_chart_with_budget(actor, kind, completion_budget())
    }
    fn complete_detection_chart_with_budget(
        &mut self,
        actor: ActorId,
        kind: Kind,
        budget: CensusBudget,
    ) -> Result<ChartCompletion, GameError> {
        if kind == Kind::Mapping {
            return self.complete_mapping_chart_with_budget(actor, budget);
        }
        let prepared = self.prepare_detection_chart(actor, kind, budget)?;
        let mut result = ChartCompletion {
            candidate: prepared.candidate,
            history_changed: false,
        };
        if kind == Kind::Monsters {
            return Ok(result);
        }
        let history_kind = if kind == Kind::Food {
            crate::chart_history::Kind::Food
        } else {
            crate::chart_history::Kind::Magic
        };
        let mut replacement = None;
        if result.admitted_nonempty() {
            let root = self.capture_historical_chart(
                history_kind,
                prepared
                    .scope
                    .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?,
                prepared.source_points,
                result
                    .candidate
                    .shared_chart()
                    .expect("positive candidate")
                    .clone(),
                crate::chart_history::archive_budget(),
            );
            match root {
                Ok(root) => replacement = Some(root),
                Err(crate::chart_history::Error::Capacity) => {
                    result.candidate = Candidate::unavailable(Unavailable::Budget)
                }
                Err(_) => return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan)),
            }
        }
        let status = if let Some(chart) = result.candidate.chart() {
            if chart.rows().is_empty() {
                crate::chart_history::AttemptStatus::Empty
            } else {
                crate::chart_history::AttemptStatus::Positive
            }
        } else {
            crate::chart_history::AttemptStatus::Unavailable(
                result
                    .candidate
                    .unavailable_reason()
                    .expect("unavailable candidate")
                    .into(),
            )
        };
        // The enclosing lifecycle publishes this candidate Game atomically.
        result.history_changed = self
            .chart_history
            .entry(actor)
            .or_default()
            .record_attempt(
                history_kind,
                crate::chart_history::Attempt {
                    attempted_at: self.tick,
                    status,
                },
                replacement,
            )
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
        Ok(result)
    }
}

// Test-only injection into the real final page-admission arithmetic, not a
// forged completion receipt or production/configurable gameplay limit.
#[cfg(test)]
thread_local! {
    static COMPLETION_PAGE_BYTES: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
fn completion_budget() -> CensusBudget {
    let budget = CensusBudget::canonical();
    #[cfg(test)]
    {
        let mut budget = budget;
        if let Some(bytes) = COMPLETION_PAGE_BYTES.with(std::cell::Cell::get) {
            budget.pages.bytes = bytes;
        }
        budget
    }
    #[cfg(not(test))]
    budget
}
#[cfg(test)]
fn with_completion_page_bytes<T>(bytes: usize, f: impl FnOnce() -> T) -> T {
    struct Reset(Option<usize>);
    impl Drop for Reset {
        fn drop(&mut self) {
            COMPLETION_PAGE_BYTES.with(|value| value.set(self.0));
        }
    }
    let _reset = Reset(COMPLETION_PAGE_BYTES.with(|value| value.replace(Some(bytes))));
    f()
}

#[cfg(test)]
#[path = "detection_tests.rs"]
mod tests;
