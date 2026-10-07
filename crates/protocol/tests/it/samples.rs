//! Recorded wire samples: one real message of every kind, for the current
//! protocol version. Each must decode and encode back to exactly the same
//! JSON, so a change to the wire format shows up as a change to the samples.
//! `scripts/record_wire_samples.py` records them from a real game; rerun it
//! when the protocol version changes and review the difference.
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::sync::Arc;
use tor_protocol::*;

fn samples() -> Value {
    let path = format!(
        "{}/tests/fixtures/wire-v{PROTOCOL_VERSION}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("{path} is missing: record it with scripts/record_wire_samples.py")
    });
    serde_json::from_str(&text).unwrap()
}

/// Decode, encode, and require the same JSON back.
fn round_trip<T: Serialize + DeserializeOwned>(sample: &Value) -> T {
    let message: T =
        serde_json::from_value(sample.clone()).unwrap_or_else(|e| panic!("{e}: {sample}"));
    assert_eq!(&serde_json::to_value(&message).unwrap(), sample);
    message
}

fn request_kind(request: &Request) -> &'static str {
    match request {
        Request::Continue => "continue",
        Request::Save => "save",
        Request::HistoryBranch { .. } => "history_branch",
        Request::Attach { .. } => "attach",
        Request::AcquireControl => "acquire_control",
        Request::ReleaseControl => "release_control",
        Request::Snapshot => "snapshot",
        Request::Palette => "palette",
        Request::Command { command, .. } => match command {
            Command::ResumeIntention { .. } => "command.resume_intention",
            Command::CancelIntention { .. } => "command.cancel_intention",
            Command::RenamePlace { .. } => "command.rename_place",
            Command::Travel { .. } => "command.travel",
            Command::Wizard { .. } => "command.wizard",
            Command::Annotate { .. } => "command.annotate",
            Command::Act { action, .. } => match action {
                Action::Attack { .. } => "act.attack",
                Action::SetDoor { .. } => "act.set_door",
                Action::Move { .. } => "act.move",
                Action::Take { .. } => "act.take",
                Action::Drop { .. } => "act.drop",
                Action::Wait => "act.wait",
            },
        },
        Request::History { .. } => "history",
    }
}

const CLIENT_KINDS: &[&str] = &[
    "hello",
    "continue",
    "save",
    "history_branch",
    "attach",
    "acquire_control",
    "release_control",
    "snapshot",
    "palette",
    "command.rename_place",
    "command.resume_intention",
    "command.cancel_intention",
    "command.travel",
    "command.wizard",
    "command.annotate",
    "act.attack",
    "act.set_door",
    "act.move",
    "act.take",
    "act.drop",
    "act.wait",
    "history",
];

fn server_kind(message: &ServerMessage) -> &'static str {
    match message {
        ServerMessage::Welcome { .. } => "welcome",
        ServerMessage::Snapshot { .. } => "snapshot",
        ServerMessage::Update { update } => match update.body {
            UpdateBody::Readiness { .. } => "update.readiness",
            UpdateBody::Intention { .. } => "update.intention",
            UpdateBody::Travel { .. } => "update.travel",
            UpdateBody::Observation { .. } => "update.observation",
            UpdateBody::ObservationDelta { .. } => "update.observation_delta",
            UpdateBody::Annotation { .. } => "update.annotation",
            UpdateBody::Control { .. } => "update.control",
        },
        ServerMessage::Ack {
            receipt: RequestReceipt::Immediate { .. },
            ..
        } => "ack.immediate",
        ServerMessage::Ack {
            receipt: RequestReceipt::Admitted { .. },
            ..
        } => "ack.admitted",
        ServerMessage::History { .. } => "history",
        ServerMessage::Error { .. } => "error",
        ServerMessage::Palette { .. } => "palette",
        ServerMessage::Waiting { .. } => "waiting",
    }
}

/// Kinds a real game sends. A full observation update is sent only when a
/// delta can't be (another branch or a resynchronization), which a short
/// recorded game doesn't reach; `update.observation` is covered by the
/// delta tests instead.
const SERVER_KINDS: &[&str] = &[
    "update.readiness",
    "update.intention",
    "welcome",
    "snapshot",
    "update.travel",
    "update.observation_delta",
    "update.annotation",
    "update.control",
    "ack.immediate",
    "ack.admitted",
    "history",
    "error",
    "palette",
];

#[test]
fn samples_are_for_the_current_protocol_version() {
    assert_eq!(samples()["protocol"], PROTOCOL_VERSION);
}

#[test]
fn every_client_message_kind_round_trips() {
    let mut kinds = BTreeSet::new();
    for sample in samples()["client"].as_array().unwrap() {
        kinds.insert(match round_trip::<ClientMessage>(sample) {
            ClientMessage::Hello { .. } => "hello",
            ClientMessage::Request { request, .. } => request_kind(&request),
        });
    }
    assert_eq!(kinds, CLIENT_KINDS.iter().copied().collect());
}

#[test]
fn every_server_message_kind_round_trips() {
    let mut kinds = BTreeSet::new();
    for sample in samples()["server"].as_array().unwrap() {
        kinds.insert(server_kind(&round_trip::<ServerMessage>(sample)));
    }
    let expected: BTreeSet<_> = SERVER_KINDS.iter().copied().collect();
    assert!(
        expected.is_subset(&kinds),
        "missing samples: {:?}",
        expected.difference(&kinds).collect::<Vec<_>>()
    );
}

#[test]
fn recorded_complete_views_validate_and_keep_projection_identity_separate() {
    let mut checked = 0;
    for sample in samples()["server"].as_array().unwrap() {
        let message: ServerMessage = serde_json::from_value(sample.clone()).unwrap();
        let mut state = match message {
            ServerMessage::Snapshot { snapshot, .. } => Arc::unwrap_or_clone(snapshot.state),
            ServerMessage::Update { update } => match update.body {
                UpdateBody::Observation { state, .. } => Arc::unwrap_or_clone(state),
                _ => continue,
            },
            _ => continue,
        };
        state.validate().unwrap();
        checked += 1;
        // Full views do not require canonical ordering.
        state.observation.visible_cells.reverse();
        state.validate().unwrap();
        if let Some(cell) = state.observation.visible_cells.first().cloned() {
            let mut image = cell.clone();
            image.position = Position {
                x: i32::MAX,
                y: i32::MAX,
                z: i32::MAX,
            };
            state.observation.visible_cells.push(image);
            state.validate().unwrap();
            state.observation.visible_cells.push(cell);
            assert_eq!(state.validate(), Err(InvalidState::DuplicateCell));
        }
    }
    assert!(checked > 0, "No recorded full views were validated");
}

#[test]
fn shared_observation_ownership_preserves_canonical_wire_bytes() {
    let mut checked = 0;
    for sample in samples()["server"].as_array().unwrap() {
        let message: ServerMessage = serde_json::from_value(sample.clone()).unwrap();
        let state = match message {
            ServerMessage::Snapshot { snapshot, .. } => snapshot.state,
            ServerMessage::Update { update } => match update.body {
                UpdateBody::Observation { state, .. } => state,
                _ => continue,
            },
            _ => continue,
        };
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(state.as_ref()).unwrap(),
            "shared ownership must not change encoded state bytes"
        );
        checked += 1;
    }
    assert!(checked > 0);
}
