//! Request completion is independent of stream repair and gameplay execution.
use crate::Connection;
use tor_protocol::{ErrorCode, RequestReceipt, ServerMessage};

/// One in-flight request. A confirmed reply remains known while its stream is
/// repaired; this object never sends, retries, or infers simulation completion.
pub struct PendingRequest {
    id: String,
    reply: Option<ConfirmedReply>,
}

pub enum RequestCompletion<'a> {
    Reply(&'a ConfirmedReply),
    /// A valid recovery snapshot arrived without a correlated reply.
    Unknown,
}

/// Only request outcomes survive recovery. Snapshot/history/palette payloads are
/// already applied or presented by their owner and are not retained a second time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmedReply {
    Receipt(RequestReceipt),
    Rejected { code: ErrorCode, message: String },
    Completed,
}

impl PendingRequest {
    pub fn new(id: String) -> Self {
        Self { id, reply: None }
    }

    /// Call after `Connection::next` applies the message. A reply alone cannot
    /// make an unsynchronized frontend ready for subsequent input.
    pub fn observe<'a>(
        &'a mut self,
        connection: &Connection,
        message: &ServerMessage,
    ) -> Option<RequestCompletion<'a>> {
        self.record(
            message,
            connection.is_synchronized(),
            connection.is_recovery_snapshot(message),
        )
    }

    fn record<'a>(
        &'a mut self,
        message: &ServerMessage,
        synchronized: bool,
        recovered: bool,
    ) -> Option<RequestCompletion<'a>> {
        let matches = match message {
            ServerMessage::Ack { request_id, .. }
            | ServerMessage::Snapshot { request_id, .. }
            | ServerMessage::History { request_id, .. } => request_id == &self.id,
            ServerMessage::Error {
                request_id: Some(request_id),
                ..
            }
            | ServerMessage::Palette {
                request_id: Some(request_id),
                ..
            } => request_id == &self.id,
            _ => false,
        };
        if matches && self.reply.is_none() {
            self.reply = Some(match message {
                ServerMessage::Ack { receipt, .. } => ConfirmedReply::Receipt(receipt.clone()),
                ServerMessage::Error { code, message, .. } => ConfirmedReply::Rejected {
                    code: *code,
                    message: message.clone(),
                },
                _ => ConfirmedReply::Completed,
            });
        }
        if !synchronized {
            return None;
        }
        match self.reply.as_ref() {
            Some(reply) => Some(RequestCompletion::Reply(reply)),
            None if recovered => Some(RequestCompletion::Unknown),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::*;

    fn reply_context() -> ReplyContext {
        ReplyContext {
            input: InputContext {
                stream: StreamContext {
                    stream: StreamId("test".into()),
                    epoch: 0,
                },
                readiness_revision: 0,
            },
            actor: ActorId(1),
            branch: BranchId("original".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: 0,
            },
            revision: 0,
        }
    }

    fn acknowledgement(id: &str) -> ServerMessage {
        ServerMessage::Ack {
            context: reply_context(),
            request_id: id.into(),
            receipt: RequestReceipt::Immediate {
                actor: ActorId(1),
                branch: BranchId("original".into()),
                entry_id: None,
            },
        }
    }

    #[test]
    fn correlated_replies_survive_repair_but_do_not_finish_before_it() {
        let replies = [
            acknowledgement("pending"),
            ServerMessage::Error {
                scope: ErrorScope::Attached {
                    context: reply_context(),
                },
                request_id: Some("pending".into()),
                code: ErrorCode::InvalidAction,
                message: "Rejected".into(),
            },
            ServerMessage::History {
                context: reply_context(),
                request_id: "pending".into(),
                page: HistoryPage {
                    entries: vec![],
                    older_before: None,
                },
            },
        ];
        for reply in replies {
            let mut pending = PendingRequest::new("pending".into());
            assert!(pending
                .record(&acknowledgement("other"), false, false)
                .is_none());
            assert!(pending.record(&reply, false, false).is_none());
            // Unrelated replies cannot replace a known original outcome.
            assert!(pending
                .record(&acknowledgement("other"), false, false)
                .is_none());
            let reset = ServerMessage::Waiting { on: Waiting::You };
            let expected = match &reply {
                ServerMessage::Ack { receipt, .. } => ConfirmedReply::Receipt(receipt.clone()),
                ServerMessage::Error { code, message, .. } => ConfirmedReply::Rejected {
                    code: *code,
                    message: message.clone(),
                },
                _ => ConfirmedReply::Completed,
            };
            assert!(matches!(pending.record(&reset, true, true),
                Some(RequestCompletion::Reply(actual)) if actual == &expected));
        }
    }

    #[test]
    fn only_a_completed_recovery_can_end_a_request_with_unknown_outcome() {
        let mut pending = PendingRequest::new("pending".into());
        let unrelated = acknowledgement("other");
        assert!(pending.record(&unrelated, false, true).is_none());
        assert!(pending.record(&unrelated, true, false).is_none());
        assert!(matches!(
            pending.record(&unrelated, true, true),
            Some(RequestCompletion::Unknown)
        ));
    }
}
