//! Immutable observer chart facts, prepared with the committed revision.
//! Query adapters never enumerate entities or select cosmetic samples.
use crate::Failure;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tor_protocol::chart_pages::*;
use tor_protocol::{ActorId, BranchId, ErrorCode};
use tor_simulation::{chart, detection::DetectionView, Game};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PreparedCharts([DetectionView; 4]);

impl PreparedCharts {
    /// The Game argument is the selected authority, never an implicit live root.
    /// Current public sessions use false; retained-boundary tests use true.
    pub(crate) fn for_game(game: &Game, actor: ActorId, historical: bool) -> Result<Self, Failure> {
        let read = |kind| {
            let mut view = game
                .detection_chart(tor_simulation::ActorId(actor.0), kind)
                .map_err(|_| Failure::new(ErrorCode::InvalidArchive, "Invalid chart observer"))?;
            view.historical |= historical;
            Ok::<_, Failure>(view)
        };
        Ok(Self([
            read(chart::Kind::Monsters)?,
            read(chart::Kind::Food)?,
            read(chart::Kind::Magic)?,
            read(chart::Kind::Mapping)?,
        ]))
    }
    pub(crate) fn summaries(
        &self,
        salt: &str,
        actor: ActorId,
        branch: &BranchId,
        revision: u64,
    ) -> Vec<ChartSummary> {
        self.0
            .iter()
            .map(|view| {
                let chart = view.candidate.as_ref().and_then(|c| c.chart());
                let status = match (&view.candidate, chart) {
                    (None, _) => ChartStatus::Inactive,
                    (_, Some(chart)) if chart.rows().is_empty() => ChartStatus::Empty,
                    (_, Some(_)) => ChartStatus::Available,
                    _ => ChartStatus::Unavailable,
                };
                ChartSummary {
                    kind: wire_kind(view.kind),
                    supported: view.supported,
                    mode: if view.historical {
                        ChartMode::Historical
                    } else {
                        ChartMode::Live
                    },
                    status,
                    acquired_at: view.acquired_at,
                    chart: chart.map(|_| token(salt, actor, branch, revision, view.kind)),
                    latest_attempt: view.latest_attempt.map(|attempt| ChartAttempt {
                        attempted_at: attempt.attempted_at,
                        status: match attempt.status {
                            tor_simulation::chart_history::AttemptStatus::Empty => {
                                ChartAttemptStatus::Empty
                            }
                            tor_simulation::chart_history::AttemptStatus::Positive => {
                                ChartAttemptStatus::Positive
                            }
                            tor_simulation::chart_history::AttemptStatus::Unavailable(_) => {
                                ChartAttemptStatus::Unavailable
                            }
                        },
                    }),
                }
            })
            .collect()
    }
    pub(crate) fn chart(&self, kind: ChartKind) -> Option<Arc<chart::Chart>> {
        self.0
            .iter()
            .find(|view| wire_kind(view.kind) == kind)?
            .candidate
            .as_ref()?
            .shared_chart()
            .cloned()
    }
}

pub(crate) fn wire_kind(kind: chart::Kind) -> ChartKind {
    match kind {
        chart::Kind::Monsters => ChartKind::Monsters,
        chart::Kind::Food => ChartKind::Food,
        chart::Kind::Magic => ChartKind::Magic,
        chart::Kind::Mapping => ChartKind::Mapping,
    }
}
fn token(
    salt: &str,
    actor: ActorId,
    branch: &BranchId,
    revision: u64,
    kind: chart::Kind,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"thresholds-chart-v1\0");
    for value in [salt.as_bytes(), branch.0.as_bytes()] {
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value);
    }
    hash.update(actor.0.to_le_bytes());
    hash.update(revision.to_le_bytes());
    hash.update([match kind {
        chart::Kind::Monsters => 0,
        chart::Kind::Food => 1,
        chart::Kind::Magic => 2,
        chart::Kind::Mapping => 3,
    }]);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(crate) fn record(chart: &chart::Chart, ordinal: usize) -> Option<ChartRecord> {
    if let Some(component) = chart.components().get(ordinal) {
        return Some(ChartRecord::Component(ChartComponent {
            component: ordinal as u64,
            alignment: match component.alignment {
                tor_simulation::scope_projection::Alignment::Aligned => ChartAlignment::Aligned,
                tor_simulation::scope_projection::Alignment::Inconsistent => {
                    ChartAlignment::Inconsistent
                }
            },
        }));
    }
    let row = chart
        .rows()
        .get(ordinal.checked_sub(chart.components().len())?)?;
    Some(ChartRecord::Row(ChartRow {
        component: row.component as u64,
        position: row.position,
        mark: match &row.mark {
            chart::Mark::Monster(appearance) => ChartMark::Monster {
                name: appearance.name.to_string(),
                asset: appearance.asset.as_deref().map(str::to_owned),
            },
            chart::Mark::Food => ChartMark::Food,
            chart::Mark::Magic => ChartMark::Magic,
            chart::Mark::Mapping(value) => ChartMark::Mapping {
                fact: match *value {
                    chart::MappingMark::WallFace { normal } => MappingMark::WallFace { normal },
                    chart::MappingMark::Passage => MappingMark::Passage,
                    chart::MappingMark::Doorway => MappingMark::Doorway,
                    chart::MappingMark::Stairs => MappingMark::Stairs,
                    chart::MappingMark::SecretDoor => MappingMark::SecretDoor,
                    chart::MappingMark::TrapPresence => MappingMark::TrapPresence,
                },
            },
        },
    }))
}

#[cfg(test)]
#[path = "chart_view_tests.rs"]
mod tests;
