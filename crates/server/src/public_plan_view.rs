//! Project only recorded observer facts; never query the current subject.
use tor_protocol as p;
use tor_simulation as s;

pub(crate) fn view(
    record: &s::public_plan::PublicPlanObservation,
    salt: &str,
    observer: s::ActorId,
) -> p::PublicPlanObservationView {
    use s::public_plan::{ObservedAim, PublicPlanState};
    p::PublicPlanObservationView {
        observed_at: record.observed_at,
        subject_name: record.subject_name.clone(),
        state: match &record.state {
            PublicPlanState::NoPublicPlan => p::PublicPlanStateView::NoPublicPlan,
            PublicPlanState::Preparing {
                ability,
                aim,
                remaining_work,
            } => p::PublicPlanStateView::Preparing {
                ability: ability.as_ref().map(|value| p::ContentRef {
                    id: value.id.to_string(),
                    name: value.name.clone(),
                }),
                aim: match aim {
                    ObservedAim::Unavailable => p::ObservedAimView::Unavailable,
                    ObservedAim::Direction { direction } => p::ObservedAimView::Direction {
                        direction: p::Position {
                            x: direction.x,
                            y: direction.y,
                            z: direction.z,
                        },
                    },
                    ObservedAim::Disclosed {
                        location,
                        occurrences,
                    } => p::ObservedAimView::Disclosed {
                        cell: crate::adapt::cell_key(salt, observer.0, *location),
                        occurrences: occurrences
                            .iter()
                            .map(|at| p::Position {
                                x: at.x,
                                y: at.y,
                                z: at.z,
                            })
                            .collect(),
                    },
                },
                remaining_work: *remaining_work,
            },
        },
    }
}
