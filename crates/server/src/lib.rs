//! Authoritative sessions, durable action/annotation history, and WebSocket transport.

mod adapt;
mod developer;
mod engine;
pub mod journal;
pub mod performance_fixture;
pub mod region_streaming;
mod regions;
pub use regions::Streaming;
pub mod scenario_package;
mod session;
mod storage;
pub use storage::{inspect_save, SavePolicy, SaveStatus};
mod transport;
pub use engine::{
    ActorSetup, BootstrapProfile, CommandProfile, CommandResult, Engine, Failure, RecoveryProfile,
    RegionCounts, Scenario,
};
pub use session::{Account, Service};
pub use transport::serve;
