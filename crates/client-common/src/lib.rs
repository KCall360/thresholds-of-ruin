//! Client-side validation of server-pushed observation ordering.

mod connection;
mod state;
pub use connection::Connection;
pub use state::{ClientState, RememberedCell};

use tor_protocol::{ActorId, StreamCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamError {
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
