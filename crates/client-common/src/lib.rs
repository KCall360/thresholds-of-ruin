//! Client-side validation of server-pushed observation ordering.

pub mod abilities;
mod connection;
pub mod inspection;
pub mod items;
mod map_memory;
pub mod narration;
mod palette;
mod pending_request;
mod state;
pub mod stats;
pub mod surfaces;
pub use connection::Connection;
pub use palette::{observation_assets, AssetTable, Palette};
pub use pending_request::{ConfirmedReply, PendingRequest, RequestCompletion};
pub use state::{ClientState, RememberedCell};

use tor_protocol::{ActorId, StreamCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
    WrongStreamContext,
    WrongObservationBase,
    WrongActor,
    SequenceMismatch,
    TimeReversed,
    WrongBranch,
    InconsistentState,
}

/// Tracks one attachment after its authoritative snapshot has been received.
///
/// This only validates ordering. `ClientState` applies payloads and `Connection`
/// supplies the transport. Reconnects establish a fresh snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationStream {
    actor: ActorId,
    cursor: StreamCursor,
}

impl ObservationStream {
    pub fn from_snapshot(actor: ActorId, cursor: StreamCursor) -> Self {
        Self { actor, cursor }
    }

    pub fn cursor(&self) -> StreamCursor {
        self.cursor
    }

    /// Accept an update atomically, leaving the cursor unchanged on failure.
    /// A sequence gap requires resynchronization before applying more updates.
    pub fn accept(&mut self, actor: ActorId, next: StreamCursor) -> Result<(), StreamError> {
        if actor != self.actor {
            return Err(StreamError::WrongActor);
        }
        if self.cursor.sequence.checked_add(1) != Some(next.sequence) {
            return Err(StreamError::SequenceMismatch);
        }
        if next.tick < self.cursor.tick {
            return Err(StreamError::TimeReversed);
        }
        self.cursor = next;
        Ok(())
    }
}

use std::future::Future;

/// The first of a server message and a local input to act on.
#[derive(Debug, PartialEq, Eq)]
pub enum FirstReady<S, L> {
    Server(S),
    Local(L),
}

/// A revision-checked command is built from local state at the moment input
/// is taken. A server message that has already arrived has to be applied
/// before that, or the command names a revision the client has already been
/// told is old. `tokio::select!` without `biased` can take the input when
/// both are ready and leave the message unread.
pub async fn server_before_local<S, L>(
    server: impl Future<Output = S>,
    local: impl Future<Output = L>,
) -> FirstReady<S, L> {
    tokio::select! {
        biased;
        message = server => FirstReady::Server(message),
        value = local => FirstReady::Local(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_delivered_message_is_taken_before_input_that_is_also_ready() {
        // Fair select picks either branch. Sixty-four trials makes that fail
        // instead of passing by chance.
        for _ in 0..64 {
            let ready = server_before_local(std::future::ready(1), std::future::ready(2)).await;
            assert_eq!(ready, FirstReady::Server(1));
        }
    }

    #[tokio::test]
    async fn input_proceeds_when_no_server_message_is_waiting() {
        let ready = server_before_local(std::future::pending::<i32>(), std::future::ready(2)).await;
        assert_eq!(ready, FirstReady::Local(2));
    }
}

pub mod combat_diagnostics;
