//! Consolidate only the administrative startup exchange, before UI input exists.
use super::{Error, Event, View, EVENT_CAPACITY};

#[derive(Default)]
pub(super) struct InitialPresentation {
    statuses: Vec<String>,
}

impl InitialPresentation {
    pub(super) fn record(&mut self, event: Event) -> Result<(), Error> {
        match event {
            // Connection has already validated and applied each server message.
            Event::View(_) => Ok(()),
            Event::Status(status) if self.statuses.len() < EVENT_CAPACITY - 2 => {
                self.statuses.push(status);
                Ok(())
            }
            Event::Status(_) => Err("Too many startup status messages".into()),
            Event::Fatal(reason) => Err(reason.into()),
            Event::Ready => Err("Unexpected readiness during startup negotiation".into()),
        }
    }

    pub(super) fn finish(self, settled: Box<View>) -> Vec<Event> {
        // Aligning the first scene clears old status: explanations must follow
        // this view. Reserve both boundary events within the bounded channel.
        std::iter::once(Event::View(settled))
            .chain(self.statuses.into_iter().map(Event::Status))
            .chain(std::iter::once(Event::Ready))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::view;
    use std::sync::mpsc;

    #[test]
    fn startup_uses_settled_state_and_preserves_explanations_after_scene_alignment() {
        let mut initial = InitialPresentation::default();
        initial.record(Event::View(Box::new(view(1)))).unwrap();
        initial
            .record(Event::Status("control refused".into()))
            .unwrap();
        initial.record(Event::View(Box::new(view(2)))).unwrap();
        initial
            .record(Event::Status("resynchronized".into()))
            .unwrap();
        let mut settled = view(3);
        settled.has_control = true;
        let shown = settled.shown.clone();
        let events = initial.finish(Box::new(settled));
        assert!(
            matches!(events.as_slice(), [Event::View(v), Event::Status(a), Event::Status(b), Event::Ready]
            if v.revision == 3 && v.has_control && v.shown == shown
            && a == "control refused" && b == "resynchronized")
        );
    }

    #[test]
    fn startup_status_capacity_reserves_view_and_ready_without_silent_eviction() {
        let mut initial = InitialPresentation::default();
        for i in 0..EVENT_CAPACITY - 2 {
            initial.record(Event::Status(i.to_string())).unwrap();
        }
        assert!(initial.record(Event::Status("overflow".into())).is_err());
        let events = initial.finish(Box::new(view(4)));
        assert_eq!(events.len(), EVENT_CAPACITY);
        for (i, event) in events[1..events.len() - 1].iter().enumerate() {
            assert!(matches!(event, Event::Status(s) if s == &i.to_string()));
        }
        let (tx, _rx) = mpsc::sync_channel(EVENT_CAPACITY);
        for event in events {
            assert!(tx.try_send(event).is_ok());
        }
        assert!(matches!(
            tx.try_send(Event::Ready),
            Err(mpsc::TrySendError::Full(_))
        ));
    }

    #[test]
    fn startup_failure_cannot_become_readiness() {
        let mut initial = InitialPresentation::default();
        assert_eq!(
            initial
                .record(Event::Fatal("failed".into()))
                .unwrap_err()
                .to_string(),
            "failed"
        );
        assert!(initial.record(Event::Ready).is_err());
    }
}
