//! Public observation types. Never expose internal world state through this crate.

mod advancement;
pub use advancement::*;

pub mod chart_pages;

pub mod map_pages;
pub use map_pages::*;

mod strike_inspection;
mod strike_pages;
pub use strike_inspection::*;
pub use strike_pages::*;
mod attack_view;
pub use attack_view::*;

mod bounded;
mod supplies;
pub use supplies::*;
mod codec;
mod combat_diagnostics;
mod combat_trace;
mod projectile_choices;
pub use projectile_choices::*;
mod projectile_selection;
mod public_plan;
pub use combat_diagnostics::*;
pub use combat_trace::*;
pub use projectile_selection::*;
pub use public_plan::*;
mod creature_catalog;
pub use creature_catalog::*;
mod creature;
mod creature_inspection;
pub use creature::*;
pub use creature_inspection::*;
mod delta;
mod integers;
mod recovery;
pub use recovery::*;
mod targets;
pub use targets::{
    ActorTarget, AppearanceSeed, DoorTarget, HoldTarget, InvalidTarget, ItemTarget, TrapTarget,
};
mod validation;
mod weapon_preview;
pub use weapon_preview::*;
mod object_presentation;
pub use object_presentation::*;
mod wire;
pub use codec::*;
pub use delta::*;
use serde::{Deserialize, Serialize};
pub use validation::*;
pub use wire::*;

/// Actor identity is explicit even in single-player sessions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActorId(#[serde(with = "crate::integers::unsigned")] pub u64);

/// Opaque host-owned attachment identity. It reveals no world topology.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StreamId(pub String);

/// One attachment and its current authoritative snapshot boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamContext {
    pub stream: StreamId,
    #[serde(with = "crate::integers::unsigned")]
    pub epoch: u64,
}

impl StreamContext {
    pub fn is_valid(&self) -> bool {
        !self.stream.0.is_empty()
            && self.stream.0.len() <= 128
            && !self.stream.0.chars().any(char::is_control)
    }

    /// Prepare a reset without mutating the current context on overflow.
    pub fn next_reset(&self) -> Option<Self> {
        Some(Self {
            stream: self.stream.clone(),
            epoch: self.epoch.checked_add(1)?,
        })
    }
}

/// Exact prior observation within the enclosing stream context and branch.
/// Control, annotation, travel and intention messages do not advance this base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationBase {
    pub cursor: StreamCursor,
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
}

/// Position in one actor's disclosed observation stream.
///
/// Sequence numbers are scoped to an attachment, not to the whole simulation.
/// Multiple updates can share a simulation tick. A new snapshot establishes a
/// new stream boundary; old-attachment messages must not enter that stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamCursor {
    #[serde(with = "crate::integers::unsigned")]
    pub sequence: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
}
