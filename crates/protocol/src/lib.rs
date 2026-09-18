//! Public observation types. Never expose internal world state through this crate.

mod wire;
use serde::{Deserialize, Serialize};
pub use wire::*;

/// Actor identity is explicit even in single-player sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActorId(pub u64);

/// Position in one actor's disclosed observation stream.
///
/// Sequence numbers are scoped to an attachment, not to the whole simulation.
/// Multiple updates can share a simulation tick. A new snapshot establishes a
/// new stream boundary; old-attachment messages must not enter that stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamCursor {
    pub sequence: u64,
    pub tick: u64,
}
