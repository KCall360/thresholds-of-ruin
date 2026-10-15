//! Text prompt boundary after a newly admitted gameplay request.
use tor_protocol::{
    ActorId, BranchId, EntryId, IntentionId, IntentionPhase, RequestReceipt, ServerMessage,
    StreamContext, UpdateBody,
};

/// Wait for this admission to leave the queue and disclose its resulting input
/// permissions. This does not wait out timed preparation or infer its outcome.
/// Call only with messages already validated and applied by the connection.
pub struct AdmissionBoundary {
    actor: ActorId,
    branch: BranchId,
    intention: IntentionId,
    entry_id: EntryId,
    execution: Option<(StreamContext, u64)>,
    execution_stopped: bool,
    permissions_received: bool,
}

impl AdmissionBoundary {
    pub fn new(receipt: &RequestReceipt) -> Option<Self> {
        let RequestReceipt::Admitted {
            actor,
            branch,
            intention,
            entry_id,
            phase: IntentionPhase::Queued,
        } = receipt
        else {
            return None;
        };
        Some(Self {
            actor: *actor,
            branch: branch.clone(),
            intention: intention.clone(),
            entry_id: entry_id.clone(),
            execution: None,
            execution_stopped: false,
            permissions_received: false,
        })
    }

    /// Ordinary gameplay waits for a stopped execution and its permissions.
    /// Keep this boundary after recovery observations that need no new permissions;
    /// a Started envelope alone must never settle a newly ready observation.
    pub fn settled(&self) -> bool {
        self.execution_stopped && self.permissions_received
    }

    pub fn observe(&mut self, message: &ServerMessage) -> bool {
        let ServerMessage::Update { update } = message else {
            return false;
        };
        if update.actor != self.actor || update.branch != self.branch {
            return false;
        }
        match &update.body {
            UpdateBody::Intention { status }
                if status.actor == self.actor
                    && status.branch == self.branch
                    && status.intention == self.intention
                    && status.entry_id == self.entry_id
                    && status.phase != IntentionPhase::Queued =>
            {
                self.execution = Some((update.context.clone(), update.cursor.sequence));
                self.execution_stopped = status.phase != IntentionPhase::Started;
                self.permissions_received = false;
                false
            }
            UpdateBody::Readiness { .. } => {
                let follows = self.execution.as_ref().is_some_and(|(stream, sequence)| {
                    *stream == update.context && update.cursor.sequence > *sequence
                });
                self.permissions_received |= follows;
                follows
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::{IntentionStatus, Readiness, StreamCursor, StreamId, StreamUpdate};

    fn receipt(phase: IntentionPhase) -> RequestReceipt {
        RequestReceipt::Admitted {
            actor: ActorId(1),
            branch: BranchId("branch".into()),
            intention: IntentionId("intention".into()),
            entry_id: EntryId("entry".into()),
            phase,
        }
    }

    fn update(sequence: u64, body: UpdateBody) -> ServerMessage {
        ServerMessage::Update {
            update: Box::new(StreamUpdate {
                context: StreamContext {
                    stream: StreamId("attachment".into()),
                    epoch: 1,
                },
                actor: ActorId(1),
                branch: BranchId("branch".into()),
                cursor: StreamCursor { sequence, tick: 0 },
                body,
            }),
        }
    }

    fn lifecycle(phase: IntentionPhase) -> ServerMessage {
        update(
            2,
            UpdateBody::Intention {
                status: IntentionStatus {
                    actor: ActorId(1),
                    branch: BranchId("branch".into()),
                    intention: IntentionId("intention".into()),
                    entry_id: EntryId("entry".into()),
                    phase,
                },
            },
        )
    }

    fn permissions(sequence: u64) -> ServerMessage {
        update(
            sequence,
            UpdateBody::Readiness {
                readiness: Readiness {
                    revision: 2,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("intention".into())],
                },
            },
        )
    }

    #[test]
    fn each_execution_phase_requires_its_later_permission_envelope() {
        for phase in [
            IntentionPhase::Started,
            IntentionPhase::Resolved,
            IntentionPhase::Failed,
            IntentionPhase::Cancelled,
            IntentionPhase::Suspended,
            IntentionPhase::Paused,
        ] {
            let mut boundary = AdmissionBoundary::new(&receipt(IntentionPhase::Queued)).unwrap();
            assert!(!boundary.observe(&permissions(1)));
            assert!(!boundary.observe(&lifecycle(IntentionPhase::Queued)));
            assert!(!boundary.observe(&permissions(3)));
            assert!(!boundary.observe(&lifecycle(phase)));
            assert!(!boundary.observe(&permissions(2)));
            assert!(boundary.observe(&permissions(3)), "{phase:?}");
            assert_eq!(boundary.settled(), phase != IntentionPhase::Started);
        }
    }

    #[test]
    fn started_permissions_do_not_settle_a_timed_action() {
        let mut boundary = AdmissionBoundary::new(&receipt(IntentionPhase::Queued)).unwrap();
        assert!(!boundary.observe(&lifecycle(IntentionPhase::Started)));
        assert!(boundary.observe(&permissions(3)));
        assert!(!boundary.settled());
        let mut resolved = lifecycle(IntentionPhase::Resolved);
        let ServerMessage::Update { update } = &mut resolved else {
            unreachable!()
        };
        update.cursor.sequence = 4;
        assert!(!boundary.observe(&resolved));
        assert!(
            !boundary.settled(),
            "earlier Started permissions do not finish resolution"
        );
        assert!(boundary.observe(&permissions(5)));
        assert!(boundary.settled());
        assert!(!boundary.observe(&ServerMessage::Waiting {
            on: tor_protocol::Waiting::You
        }));
        assert!(
            boundary.settled(),
            "later readiness observations need not change permissions"
        );
    }

    #[test]
    fn only_the_exact_admission_and_stream_can_finish_the_boundary() {
        for different in 0..5 {
            let mut boundary = AdmissionBoundary::new(&receipt(IntentionPhase::Queued)).unwrap();
            let mut unrelated = lifecycle(IntentionPhase::Resolved);
            let ServerMessage::Update { update } = &mut unrelated else {
                unreachable!()
            };
            let UpdateBody::Intention { status } = &mut update.body else {
                unreachable!()
            };
            match different {
                0 => update.actor = ActorId(2),
                1 => update.branch = BranchId("other".into()),
                2 => status.actor = ActorId(2),
                3 => status.intention = IntentionId("other".into()),
                _ => status.entry_id = EntryId("other".into()),
            }
            assert!(!boundary.observe(&unrelated));
            assert!(!boundary.observe(&permissions(3)));
        }
        let mut boundary = AdmissionBoundary::new(&receipt(IntentionPhase::Queued)).unwrap();
        assert!(!boundary.observe(&lifecycle(IntentionPhase::Resolved)));
        let mut reset = permissions(3);
        let ServerMessage::Update { update } = &mut reset else {
            unreachable!()
        };
        update.context.epoch += 1;
        assert!(!boundary.observe(&reset));
        assert!(boundary.observe(&permissions(3)));
    }

    #[test]
    fn immediate_and_previously_executed_receipts_need_no_new_execution() {
        assert!(AdmissionBoundary::new(&RequestReceipt::Immediate {
            actor: ActorId(1),
            branch: BranchId("branch".into()),
            entry_id: None,
        })
        .is_none());
        for phase in [
            IntentionPhase::Started,
            IntentionPhase::Resolved,
            IntentionPhase::Failed,
        ] {
            assert!(AdmissionBoundary::new(&receipt(phase)).is_none());
        }
    }
}
