//! Optional private process log. Live publication is not durable save admission.
//! A verifier requires contiguous history and a matching, loss-free save barrier.
use super::*;
use serde_json::json;

pub(super) struct SurvivalLog {
    sink: crate::diagnostics::Diagnostics,
    segment: String,
}
impl std::fmt::Debug for SurvivalLog {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SurvivalLog")
            .field("segment", &self.segment)
            .finish_non_exhaustive()
    }
}
impl Engine {
    /// Enable a private process diagnostic after restore. Never replays old flows.
    /// Failure to open the sink is independent of simulation success.
    pub fn enable_survival_diagnostics(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        self.survival_log = Some(SurvivalLog {
            sink: crate::diagnostics::Diagnostics::survival_file(path.as_ref())?,
            segment: Uuid::new_v4().to_string(),
        });
        self.emit_survival(json!({"event":"survival.begin"}));
        Ok(())
    }
    pub(super) fn emit_survival(&self, mut record: serde_json::Value) {
        let Some(log) = &self.survival_log else {
            return;
        };
        record["survival_version"] = 1.into();
        record["segment"] = log.segment.clone().into();
        record["lineage"] = self.archive.branch.0.clone().into();
        record["branch"] = self.current_branch.0.clone().into();
        record["history_sequence"] = self.archive.records.len().into();
        record["entry_id"] = json!(self.archive.records.last().map(|record| &record.entry.id.0));
        record["selected_actor"] = json!(self.selected_character().map(|actor| actor.0));
        record["tick"] = self.game.tick().into();
        log.sink.survival(record);
    }
    /// A successful save acknowledges durability separately from live publication.
    /// The async diagnostic queue is not flushed by the gameplay save barrier.
    pub(crate) fn survival_save_barrier(&self, durable_sequence: u64) {
        self.emit_survival(
            json!({"event":"survival.barrier", "durable_sequence":durable_sequence}),
        );
    }
}
