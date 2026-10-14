//! Authoritative sessions, durable action/annotation history, and WebSocket transport.

mod actions;
mod adapt;
mod advancement_adapter;
mod advancement_page;
#[cfg(test)]
mod advancement_test_support;
pub mod creature_authoring;
mod creature_view;
mod developer;
mod diagnostics;
mod engine;
pub mod generation_recipe;
pub mod generator;
mod history_index;
mod outbound;
pub mod procedural_world;
pub use outbound::OutboundLimits;
pub mod journal;
pub mod performance_fixture;
mod preload;
#[doc(hidden)]
pub mod process_diagnostics;
mod public_plan_view;
pub mod region_streaming;
mod regions;
mod runner;
pub use regions::{RegionAcquisitionProfile, Streaming};
pub mod scenario_package;
mod session;
mod storage;
pub use storage::{inspect_save, SavePolicy, SaveStatus};
mod transport;
pub mod wire_adapter;
pub use engine::{
    ActorSetup, BootstrapProfile, CommandProfile, CommandResult, Engine, Failure, RecoveryProfile,
    RegionCounts, Scenario,
};
pub use runner::{Simulation, SimulationHandle};
pub use session::{Account, Service};
pub use transport::serve;

pub use engine::arena_evaluation;

#[cfg(test)]
#[path = "../test_support/packages.rs"]
mod test_packages;

#[cfg(test)]
use crate as host;
#[cfg(test)]
#[path = "../test_support/public_plan.rs"]
mod public_plan_test_fixture;
