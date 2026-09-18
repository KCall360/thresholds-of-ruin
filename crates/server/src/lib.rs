//! Authoritative sessions, durable action/annotation history, and WebSocket transport.

mod adapt;
mod engine;
mod session;
mod transport;
pub use engine::{ActorSetup, CommandResult, Engine, Failure, Scenario};
pub use session::{Account, Service};
pub use transport::serve;
