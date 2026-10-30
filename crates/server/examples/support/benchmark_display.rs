//! Explicit causal delivery for direct-Engine diagnostic fixtures.
use tor_protocol::{
    EventProvenance, HistoryContent, HistoryEntry, Observation, OutcomeFacts, UpdateBody,
};

pub fn outcome(
    before: &Observation,
    after: &Observation,
    before_stamp: EventProvenance,
    after_stamp: EventProvenance,
    event: Option<HistoryEntry>,
) -> UpdateBody {
    let action = event.as_ref().and_then(|entry| match &entry.content {
        HistoryContent::Action { event, .. } => Some(event.as_ref()),
        _ => None,
    });
    let facts = OutcomeFacts::capture_causal(before, after, action, before_stamp, after_stamp)
        .expect("diagnostic captures every causal transition");
    UpdateBody::Outcome {
        facts,
        event: event.map(Box::new),
        visual_changed: OutcomeFacts::picture_changed(before, after),
    }
}
