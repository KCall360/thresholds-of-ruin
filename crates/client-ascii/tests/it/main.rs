//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod linked_holds;
mod magic_items;
mod map_memory;
mod presentation;
mod public_plan_observation;
mod turns_and_map;

fn request(effect: tor_client_ascii::Effect) -> Option<tor_protocol::Request> {
    match effect {
        tor_client_ascii::Effect::Request(request) => Some(*request),
        _ => None,
    }
}

// Synthetic disclosed references for client-model fixtures, not server derivation.
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

mod supply_selection;

mod advancement;

mod charts;
