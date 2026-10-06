//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod map_memory;
mod memory;
mod palette;
mod recovery;
mod streamed_notes;
mod travel;
mod validation;

fn stream_context(epoch: u64) -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("3b7523b8-893a-4ea9-8b09-0a3887a7e6a1".into()),
        epoch,
    }
}

// Existing payload tests explicitly establish a fresh snapshot boundary.
fn reset_snapshot(
    client: &mut tor_client_common::ClientState,
    mut snapshot: tor_protocol::Snapshot,
) -> Result<(), tor_client_common::StreamError> {
    snapshot.context = tor_protocol::StreamContext {
        stream: client.context().stream.clone(),
        epoch: client.context().epoch + 1,
    };
    client.replace_snapshot(snapshot)
}
