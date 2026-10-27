//! Immutable Mapping acquisition, separate from ordinary perception/navigation.
use crate::chart::{self, Candidate, Unavailable};
use crate::chart_history::{self, AcquisitionAuthority, Attempt, AttemptStatus, HistoricalChart};
use crate::detection::{CensusBudget, ChartCompletion, DetectionView};
use crate::mapping_chart::{prepare_scope_mapping, SourceRow};
use crate::{ActorId, Game, GameError};
use tor_world::{Shared, World};

pub(crate) fn validate_mapping_history(
    world: &World,
    scope: &crate::spatial_scope::registry::Scope,
    chart: &chart::Chart,
    source_rows: &[SourceRow],
    budget: chart_history::ValidationBudget,
) -> Result<(), chart_history::Error> {
    let prepared = prepare_scope_mapping(
        world,
        scope,
        CensusBudget {
            work: budget.projection.work,
            allocation_bytes: budget.projection.allocation_bytes,
            delivery_bytes: budget.chart.json_bytes,
            pages: crate::chart::delivery::PageLimits::canonical(),
        },
    );
    let Some(expected) = prepared.candidate.chart() else {
        return Err(match prepared.candidate.unavailable_reason() {
            Some(Unavailable::Budget) => chart_history::Error::Capacity,
            _ => chart_history::Error::Source,
        });
    };
    if prepared.scope.as_deref() != Some(scope.id.as_str())
        || expected != chart
        || prepared.source_rows != source_rows
    {
        return Err(chart_history::Error::Projection);
    }
    Ok(())
}
impl Game {
    pub(crate) fn mapping_chart(&self, actor: ActorId) -> Result<DetectionView, GameError> {
        if !self.actors.contains_key(&actor) {
            return Err(GameError::UnknownActor);
        }
        let slot = self
            .chart_history
            .get(&actor)
            .map(|owner| owner.get(chart_history::Kind::Mapping));
        let latest = slot.and_then(|slot| slot.latest());
        let supported = self.mapping_supported(actor);
        Ok(DetectionView {
            kind: chart::Kind::Mapping,
            supported,
            historical: true,
            acquired_at: latest.map(|root| root.acquired_tick()),
            candidate: latest
                .map(|root| Candidate::from_shared_chart(root.shared_chart().clone()))
                .or_else(|| (!supported).then(|| Candidate::unavailable(Unavailable::Projection))),
            latest_attempt: slot.and_then(|slot| slot.attempt()),
        })
    }
    pub(crate) fn complete_mapping_chart_with_budget(
        &mut self,
        actor: ActorId,
        budget: CensusBudget,
    ) -> Result<ChartCompletion, GameError> {
        let prepared = self.prepare_mapping_chart(actor, budget)?;
        let mut result = ChartCompletion {
            candidate: prepared.candidate,
            history_changed: false,
        };
        let mut replacement = None;
        if result.admitted_nonempty() {
            let scope = prepared
                .scope
                .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
            let captured = HistoricalChart::checked_mapping(
                self.tick,
                scope,
                Shared::new(AcquisitionAuthority {
                    world: self.world.clone(),
                    registry: self.spatial_registry.clone(),
                }),
                result
                    .candidate
                    .shared_chart()
                    .expect("nonempty candidate")
                    .clone(),
                prepared.source_rows,
                chart_history::archive_budget(),
            );
            match captured {
                Ok(root) => replacement = Some(Shared::new(root)),
                Err(chart_history::Error::Capacity) => {
                    result.candidate = Candidate::unavailable(Unavailable::Budget)
                }
                Err(_) => return Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan)),
            }
        }
        let status = match result.candidate.chart() {
            Some(chart) if chart.rows().is_empty() => AttemptStatus::Empty,
            Some(_) => AttemptStatus::Positive,
            None => AttemptStatus::Unavailable(
                result
                    .candidate
                    .unavailable_reason()
                    .expect("unavailable")
                    .into(),
            ),
        };
        result.history_changed = self
            .chart_history
            .entry(actor)
            .or_default()
            .record_attempt(
                chart_history::Kind::Mapping,
                Attempt {
                    attempted_at: self.tick,
                    status,
                },
                replacement,
            )
            .map_err(|_| GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
        Ok(result)
    }
}
