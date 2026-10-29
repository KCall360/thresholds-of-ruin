//! Opening a new overview is a two-stage user operation, not a wire retry.
//! Runners present every message during Observe using their normal event pump.
use tor_protocol::{ReplyContext, Request};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenOverview {
    origin: ReplyContext,
}

impl OpenOverview {
    pub fn new(state: &crate::ClientState) -> Result<Self, String> {
        if !state.has_control() {
            return Err("Advancement requires control of your character.".into());
        }
        Ok(Self {
            origin: state.reply_context(),
        })
    }

    pub fn observation_request(&self) -> Request {
        Request::Observe
    }

    /// Validate before constructing the new basis-free Overview request.
    /// Page navigation and confirmations never use this operation.
    pub fn finish(self, state: &crate::ClientState) -> Result<(), String> {
        if !state.observation_current() {
            return Err("Observation changed again. Open advancement again.".into());
        }
        self.finish_at(&state.reply_context(), state.has_control())
    }

    pub fn validate_attachment(&self, state: &crate::ClientState) -> Result<(), String> {
        self.validate_at(&state.reply_context(), state.has_control())
    }

    fn validate_at(&self, current: &ReplyContext, controlled: bool) -> Result<(), String> {
        if !controlled
            || current.actor != self.origin.actor
            || current.branch != self.origin.branch
            || current.input.stream != self.origin.input.stream
        {
            return Err("Attachment or control changed. Open advancement again.".into());
        }
        Ok(())
    }

    fn finish_at(self, current: &ReplyContext, controlled: bool) -> Result<(), String> {
        self.validate_at(current, controlled)?;
        if current.snapshot == self.origin.snapshot {
            return Err("Observation did not complete. Open advancement again.".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::*;

    fn context() -> ReplyContext {
        ReplyContext {
            snapshot: SnapshotId("before".into()),
            input: InputContext {
                stream: StreamContext {
                    stream: StreamId("stream".into()),
                    epoch: 1,
                },
                readiness_revision: 2,
            },
            actor: ActorId(1),
            branch: BranchId("branch".into()),
            cursor: StreamCursor {
                sequence: 3,
                tick: 4,
            },
            revision: 5,
        }
    }

    #[test]
    fn overview_requires_completed_picture_but_accepts_new_disclosed_progress() {
        let origin = context();
        assert!(OpenOverview {
            origin: origin.clone()
        }
        .finish_at(&origin, true)
        .is_err());
        let mut current = origin.clone();
        current.snapshot = SnapshotId("after".into());
        current.revision += 1;
        current.cursor.sequence += 3;
        current.input.readiness_revision += 1;
        assert!(OpenOverview { origin }.finish_at(&current, true).is_ok());
    }

    #[test]
    fn overview_does_not_cross_reset_actor_branch_or_lost_control() {
        for change in 0..5 {
            let origin = context();
            let mut current = origin.clone();
            current.snapshot = SnapshotId("after".into());
            match change {
                0 => current.input.stream.epoch += 1,
                1 => current.input.stream.stream = StreamId("other".into()),
                2 => current.actor = ActorId(2),
                3 => current.branch = BranchId("rewound".into()),
                _ => {}
            }
            assert!(OpenOverview { origin }
                .finish_at(&current, change != 4)
                .is_err());
        }
    }

    #[test]
    fn later_dirty_outcome_invalidates_an_otherwise_completed_picture() {
        let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
            "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
            "context":{"stream":"overview-fixture","epoch":"0"},
            "actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},
            "has_control":true,"intentions":[],
            "history":{"entries":[],"older_before":null},
            "state":{"snapshot":"before","wizard_game":false,"revision":"1",
                     "observation":crate::test_support::observation()}
        }))
        .unwrap();
        let mut state = crate::ClientState::from_snapshot(snapshot).unwrap();
        let plan = OpenOverview::new(&state).unwrap();
        let mut picture = state.state().clone();
        picture.snapshot = SnapshotId("after".into());
        picture.revision += 1;
        state
            .apply(StreamUpdate {
                context: state.context().clone(),
                actor: state.actor(),
                branch: state.branch().clone(),
                cursor: StreamCursor {
                    sequence: 1,
                    tick: 0,
                },
                body: UpdateBody::Observation {
                    state: picture.into(),
                    event: None,
                },
            })
            .unwrap();
        assert!(plan.clone().finish(&state).is_ok());
        let before = state.state().observation.clone();
        let mut after = before.clone();
        after.tick = 1;
        state
            .apply(StreamUpdate {
                context: state.context().clone(),
                actor: state.actor(),
                branch: state.branch().clone(),
                cursor: StreamCursor {
                    sequence: 2,
                    tick: 1,
                },
                body: UpdateBody::Outcome {
                    visual_changed: true,
                    facts: OutcomeFacts::capture(&before, &after, Some(&Event::Waited)),
                    event: None,
                },
            })
            .unwrap();
        assert!(!state.observation_current());
        assert!(plan.finish(&state).unwrap_err().contains("changed again"));
    }
}
