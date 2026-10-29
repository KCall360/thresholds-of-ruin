//! Attachment-local picture capabilities. Only admitted output creates authority.
//!
//! The latest full picture lives on `Client` for delta encoding. Older entries
//! retain only the exact base, advertised chart capabilities and immutable rules
//! boundary needed by pages and annotations; they do not pin old `StateView`s.
use super::*;
use tor_protocol::chart_pages::ChartSummary;

const RETAINED_PRESENTATIONS: usize = 32;

pub(super) struct IssuedPresentation {
    context: StreamContext,
    actor: ActorId,
    pub(super) branch: BranchId,
    role: AccessRole,
    pub(super) base: ObservationBase,
    pub(super) charts: Vec<ChartSummary>,
    pub(super) boundary: crate::engine::PresentationBoundary,
}

impl IssuedPresentation {
    fn belongs_to(&self, client: &Client, branch: &BranchId) -> bool {
        client.context.as_ref() == Some(&self.context)
            && client.actor == Some(self.actor)
            && client.role == self.role
            && branch == &self.branch
    }
}

/// The generation and bounded capability set have one commit point, after the
/// outbound queue accepts a picture. Preparing or rejecting output changes none
/// of them. A recovery reset invalidates older capabilities but not generations.
#[derive(Default)]
pub(super) struct PresentationHistory {
    generation: u64,
    entries: VecDeque<IssuedPresentation>,
}

impl PresentationHistory {
    fn next_generation(&self) -> Result<u64, Failure> {
        self.generation
            .checked_add(1)
            .ok_or_else(|| Failure::new(ErrorCode::ResourceLimit, "Display generation exhausted"))
    }

    fn commit_admitted(&mut self, issued: IssuedPresentation, reset: bool) {
        debug_assert_eq!(self.generation.checked_add(1), Some(issued.base.revision));
        if reset {
            self.entries.clear();
        }
        self.generation = issued.base.revision;
        self.entries.push_back(issued);
        while self.entries.len() > RETAINED_PRESENTATIONS {
            self.entries.pop_front();
        }
    }

    #[cfg(test)]
    pub(super) fn fixture_entry_mut(&mut self, base: &ObservationBase) -> &mut IssuedPresentation {
        self.entries
            .iter_mut()
            .find(|issued| &issued.base == base)
            .expect("fixture must retain the exact admitted capability")
    }
}

fn stale() -> Failure {
    Failure::new(ErrorCode::StaleContext, "Snapshot is no longer available")
}

impl Service {
    pub(super) fn issued_presentation(
        &self,
        id: u64,
        base: &ObservationBase,
    ) -> Result<&IssuedPresentation, Failure> {
        let client = self.clients.get(&id).ok_or_else(stale)?;
        client
            .presentations
            .entries
            .iter()
            .find(|issued| &issued.base == base && issued.belongs_to(client, self.engine.branch()))
            .ok_or_else(stale)
    }

    pub(super) fn presentation_by_snapshot(
        &self,
        id: u64,
        snapshot: &SnapshotId,
    ) -> Result<&IssuedPresentation, Failure> {
        let client = self.clients.get(&id).ok_or_else(stale)?;
        client
            .presentations
            .entries
            .iter()
            .find(|issued| {
                &issued.base.snapshot == snapshot && issued.belongs_to(client, self.engine.branch())
            })
            .ok_or_else(stale)
    }

    pub(super) fn prepare_presentation(
        &self,
        id: u64,
    ) -> Result<(crate::engine::PresentationBoundary, StateView), Failure> {
        let client = self
            .clients
            .get(&id)
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let actor = client
            .actor
            .ok_or_else(|| Failure::new(ErrorCode::NotAttached, "Attach an actor first"))?;
        let generation = client.presentations.next_generation()?;
        let boundary = self.engine.presentation_boundary();
        let mut state = self.engine.state_at(&boundary, actor, generation)?;
        state.snapshot = SnapshotId(uuid::Uuid::new_v4().to_string());
        restrict_item_disclosure(&mut state, client.role);
        Ok((boundary, state))
    }

    /// Called only after `last_observation` was committed by admitted output.
    pub(super) fn commit_admitted_presentation(
        &mut self,
        id: u64,
        boundary: crate::engine::PresentationBoundary,
        reset: bool,
    ) {
        let client = self.clients.get_mut(&id).expect("admitted attachment");
        let observed = client.last_observation.as_ref().expect("admitted picture");
        let issued = IssuedPresentation {
            context: client.context.clone().expect("attached stream"),
            actor: client.actor.expect("attached actor"),
            branch: observed.branch.clone(),
            role: client.role,
            base: observed.base.clone(),
            charts: observed.state.observation.charts.clone(),
            boundary,
        };
        client.presentations.commit_admitted(issued, reset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attached() -> (Service, Connection) {
        let mut service = Service::new(Engine::memory(crate::Scenario::two_room(42)).unwrap());
        let actor = ActorId(1);
        let mut client = service
            .connect(
                &Account {
                    role: AccessRole::Spectator,
                    user: "picture-reader".into(),
                    token: "test-only".into(),
                    actors: BTreeSet::from([actor]),
                },
                "headless".into(),
            )
            .unwrap();
        service.handle(client.id, "attach".into(), Request::Attach { actor });
        while client.messages.try_recv_frame().is_ok() {}
        (service, client)
    }

    #[test]
    fn history_retains_bounded_capabilities_without_old_full_pictures() {
        let (mut service, mut client) = attached();
        let first = service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap();
        let first_base = first.base.clone();
        let old_picture = Arc::downgrade(&first.state);
        service.observe(client.id).unwrap();
        while client.messages.try_recv_frame().is_ok() {}
        assert!(service.issued_presentation(client.id, &first_base).is_ok());
        assert!(
            old_picture.upgrade().is_none(),
            "retained capabilities must not pin full pictures"
        );
        for _ in 1..RETAINED_PRESENTATIONS {
            service.observe(client.id).unwrap();
            while client.messages.try_recv_frame().is_ok() {}
        }
        let history = &service.clients[&client.id].presentations;
        assert_eq!(history.entries.len(), RETAINED_PRESENTATIONS);
        assert_eq!(history.generation, RETAINED_PRESENTATIONS as u64 + 1);
        assert_eq!(
            service
                .issued_presentation(client.id, &first_base)
                .err()
                .unwrap()
                .code,
            ErrorCode::StaleContext
        );
        let latest = service.clients[&client.id]
            .last_observation
            .as_ref()
            .unwrap();
        assert!(service.issued_presentation(client.id, &latest.base).is_ok());
        service.snapshot(client.id, "reset").unwrap();
        let history = &service.clients[&client.id].presentations;
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.generation, RETAINED_PRESENTATIONS as u64 + 2);
    }

    #[test]
    fn preparation_and_rejected_output_do_not_admit_capabilities() {
        let (mut service, client) = attached();
        let id = client.id;
        let original = service.clients[&id]
            .last_observation
            .as_ref()
            .unwrap()
            .base
            .clone();
        let (_, first) = service.prepare_presentation(id).unwrap();
        let (_, second) = service.prepare_presentation(id).unwrap();
        assert_eq!(first.revision, original.revision + 1);
        assert_eq!(second.revision, first.revision);
        assert_ne!(first.snapshot, second.snapshot);
        let history = &service.clients[&id].presentations;
        assert_eq!(history.generation, original.revision);
        assert_eq!(history.entries.len(), 1);
        assert_eq!(
            service
                .presentation_by_snapshot(id, &first.snapshot)
                .err()
                .unwrap()
                .code,
            ErrorCode::StaleContext
        );
        let boundary = service.engine.causal_boundary(ActorId(1)).unwrap();
        drop(client);
        service.observe(id).unwrap();
        assert!(
            !service.clients.contains_key(&id),
            "failed output must discard attachment authority"
        );
        assert_eq!(
            service.engine.causal_boundary(ActorId(1)).unwrap(),
            boundary
        );
        assert_eq!(
            service
                .presentation_by_snapshot(id, &original.snapshot)
                .err()
                .unwrap()
                .code,
            ErrorCode::StaleContext
        );
    }
}
