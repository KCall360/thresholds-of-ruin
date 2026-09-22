//! Authoritative sessions, durable action/annotation history, and WebSocket transport.

mod adapt;
mod developer;
mod engine;
pub mod journal;
mod session;
mod transport;
pub use engine::{ActorSetup, CommandProfile, CommandResult, Engine, Failure, Scenario};
pub use session::{Account, Service};
pub use transport::serve;
