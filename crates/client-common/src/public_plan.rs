//! Presentation of already-paid historical facts; no target resolution or clock.
use std::fmt::Write;
use tor_protocol::{ObservedAimView, PublicPlanObservationView, PublicPlanStateView};

pub fn line(record: &PublicPlanObservationView) -> String {
    let mut text = format!(
        "Observed at tick {}: {} ",
        record.observed_at, record.subject_name
    );
    match &record.state {
        PublicPlanStateView::NoPublicPlan => text.push_str("had no public plan available"),
        PublicPlanStateView::Preparing {
            ability,
            aim,
            remaining_work,
        } => {
            text.push_str("was preparing");
            if let Some(ability) = ability {
                text.push(' ');
                text.push_str(&ability.name);
            } else {
                text.push_str("; ability undisclosed");
            }
            text.push_str("; aim ");
            match aim {
                ObservedAimView::Unavailable => text.push_str("unavailable"),
                ObservedAimView::Disclosed { occurrences, .. } => {
                    for (index, at) in occurrences.iter().enumerate() {
                        if index > 0 {
                            text.push_str(" / ");
                        }
                        write!(&mut text, "({},{},{})", at.x, at.y, at.z)
                            .expect("String writes are infallible");
                    }
                    text.push_str(" in your frame at that time");
                }
            }
            write!(&mut text, "; {remaining_work} work remaining")
                .expect("String writes are infallible");
        }
    }
    text.push('.');
    text
}

/// A repeated read, occlusion or later clock change never creates a new result.
pub fn changed_line(
    before: Option<&PublicPlanObservationView>,
    after: Option<&PublicPlanObservationView>,
) -> Option<String> {
    after.filter(|record| before != Some(*record)).map(line)
}
