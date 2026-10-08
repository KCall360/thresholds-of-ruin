//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod adventure;
mod commands;
mod parser;
mod turns;

// Synthetic entity references for client-model tests, not server target derivation.
fn fixture_digest(index: u64) -> [u8; 32] {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&index.to_le_bytes());
    digest
}
fn actor_target(index: u64) -> tor_protocol::ActorTarget {
    tor_protocol::ActorTarget::from_digest(fixture_digest(index))
}
fn item_target(index: u64) -> tor_protocol::ItemTarget {
    tor_protocol::ItemTarget::from_digest(fixture_digest(index))
}
fn door_target(index: u64) -> tor_protocol::DoorTarget {
    tor_protocol::DoorTarget::from_digest(fixture_digest(index))
}
