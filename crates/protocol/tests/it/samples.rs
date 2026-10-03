//! Recorded wire samples: one real message of every kind, for the current
//! protocol version. Each must decode and encode back to exactly the same
//! JSON, so a change to the wire format shows up as a change to the samples.
//! `scripts/record_wire_samples.py` records them from a real game; rerun it
//! when the protocol version changes and review the difference.
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
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
            UpdateBody::Travel { .. } => "update.travel",
            UpdateBody::Observation { .. } => "update.observation",
            UpdateBody::ObservationDelta { .. } => "update.observation_delta",
            UpdateBody::Annotation { .. } => "update.annotation",
            UpdateBody::Control { .. } => "update.control",
        },
        ServerMessage::Ack { .. } => "ack",
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
    "welcome",
    "snapshot",
    "update.travel",
    "update.observation_delta",
    "update.annotation",
    "update.control",
    "ack",
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
