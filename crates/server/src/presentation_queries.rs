//! Read-only queries bind the latest issued picture, never a private rules ordinal.
use super::*;

impl Service {
    pub(super) fn query_presentation(
        &self,
        id: u64,
        expected_revision: u64,
    ) -> Result<&IssuedPresentation, Failure> {
        let stale = || {
            Failure::new(
                ErrorCode::StaleContext,
                "Refresh the observation before inspecting",
            )
        };
        let client = self.clients.get(&id).ok_or_else(stale)?;
        let observed = client.last_observation.as_ref().ok_or_else(stale)?;
        if observed.base.revision != expected_revision {
            return Err(stale());
        }
        self.issued_presentation(id, &observed.base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_basis_tracks_issued_picture_not_intervening_rules_boundary() {
        let mut service = Service::new(Engine::memory(crate::Scenario::two_room(42)).unwrap());
        let actor = ActorId(1);
        let mut client = service
            .connect(
                &Account {
                    role: AccessRole::Spectator,
                    user: "frozen-query".into(),
                    token: "test-only".into(),
                    actors: BTreeSet::from([actor]),
                },
                "headless".into(),
            )
            .unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv_frame().is_ok() {}
        let base = service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap()
            .base
            .clone();
        let causal = service.engine.causal_boundary(actor).unwrap();
        service
            .engine
            .command(
                "test",
                "test",
                actor,
                "rules-only",
                &service.engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: causal,
                    action: crate::actions::Action::Wait,
                },
            )
            .unwrap();
        assert!(service.engine.causal_boundary(actor).unwrap() > causal);
        assert_eq!(
            service
                .query_presentation(client.id, base.revision)
                .unwrap()
                .base,
            base
        );
        service.handle(client.id, "picture".into(), Request::Observe);
        let latest = service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap()
            .base
            .clone();
        assert!(latest.revision > base.revision);
        assert_ne!(latest.snapshot, base.snapshot);
        assert_eq!(
            service
                .query_presentation(client.id, base.revision)
                .err()
                .unwrap()
                .code,
            ErrorCode::StaleContext
        );
        assert_eq!(
            service
                .query_presentation(client.id, latest.revision)
                .unwrap()
                .base,
            latest
        );
        // Old immutable pages may still use the retained capability; query menus
        // explicitly require the latest picture and do not substitute live facts.
        assert!(service.issued_presentation(client.id, &base).is_ok());
    }
}
