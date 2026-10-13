//! Recorded wire samples cover every required kind for the current protocol.
//! Intentionally unrecorded kinds are explicit in `server_sample_kinds!`. Each
//! sample must decode and encode back to exactly the same JSON, so a wire-format
//! change shows up as a change to the samples.
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
            Command::InspectStrikeSource { .. } => "command.inspect_strike_source",
            Command::ResumeIntention { .. } => "command.resume_intention",
            Command::CancelIntention { .. } => "command.cancel_intention",
            Command::RenamePlace { .. } => "command.rename_place",
            Command::Travel { .. } => "command.travel",
            Command::Wizard { .. } => "command.wizard",
            Command::Annotate { .. } => "command.annotate",
            Command::Act { action, .. } => match action {
                Action::UseAbility { .. } => "act.use_ability",
                Action::Equip { .. } => "act.equip",
                Action::Unequip { .. } => "act.unequip",
                Action::Drink { .. } => "act.drink",
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
    "command.inspect_strike_source",
    "command.travel",
    "command.wizard",
    "command.annotate",
    "act.equip",
    "act.unequip",
    "act.drink",
    "act.use_ability",
    "act.set_door",
    "act.move",
    "act.take",
    "act.drop",
    "act.wait",
    "history",
];

// One declaration owns the complete kind set and its recording policy. The
// exhaustive message match below must classify every server enum variant; adding
// a kind here also adds it to ALL, so coverage cannot drift from a second list.
macro_rules! server_sample_kinds {
    ($($kind:ident => ($name:literal, $exclusion:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        enum ServerSampleKind { $($kind),+ }

        impl ServerSampleKind {
            const ALL: &'static [Self] = &[$(Self::$kind),+];

            fn recording(self) -> (&'static str, Option<&'static str>) {
                match self { $(Self::$kind => ($name, $exclusion)),+ }
            }
        }
    };
}

server_sample_kinds! {
    Welcome => ("welcome", None),
    Snapshot => ("snapshot", None),
    SnapshotPart => ("snapshot_part", None),
    Readiness => ("update.readiness", None),
    Intention => ("update.intention", None),
    Travel => ("update.travel", None),
    Observation => (
        "update.observation",
        Some("The short recording does not reach a branch reset or resynchronization. Full observation encoding is covered by delta.rs.")
    ),
    ObservationDelta => ("update.observation_delta", None),
    Annotation => ("update.annotation", None),
    Control => ("update.control", None),
    ImmediateAck => ("ack.immediate", None),
    AdmittedAck => ("ack.admitted", None),
    History => ("history", None),
    CreatureInspection => ("creature_inspection", None),
    CreatureStrikePage => ("creature_strike_page", None),
    CombatDiagnostics => ("combat_diagnostics", None),
    Error => ("error", None),
    Palette => ("palette", None),
    Waiting => ("waiting", None),
}

fn server_kind(message: &ServerMessage) -> ServerSampleKind {
    use ServerSampleKind as Kind;
    match message {
        ServerMessage::Welcome { .. } => Kind::Welcome,
        ServerMessage::Snapshot { .. } => Kind::Snapshot,
        ServerMessage::SnapshotPart { .. } => Kind::SnapshotPart,
        ServerMessage::Update { update } => match update.body {
            UpdateBody::Readiness { .. } => Kind::Readiness,
            UpdateBody::Intention { .. } => Kind::Intention,
            UpdateBody::Travel { .. } => Kind::Travel,
            UpdateBody::Observation { .. } => Kind::Observation,
            UpdateBody::ObservationDelta { .. } => Kind::ObservationDelta,
            UpdateBody::Annotation { .. } => Kind::Annotation,
            UpdateBody::Control { .. } => Kind::Control,
        },
        ServerMessage::Ack {
            receipt: RequestReceipt::Immediate { .. },
            ..
        } => Kind::ImmediateAck,
        ServerMessage::Ack {
            receipt: RequestReceipt::Admitted { .. },
            ..
        } => Kind::AdmittedAck,
        ServerMessage::History { .. } => Kind::History,
        ServerMessage::CreatureInspection { .. } => Kind::CreatureInspection,
        ServerMessage::CreatureStrikePage { .. } => Kind::CreatureStrikePage,
        ServerMessage::CombatDiagnostics { .. } => Kind::CombatDiagnostics,
        ServerMessage::Error { .. } => Kind::Error,
        ServerMessage::Palette { .. } => Kind::Palette,
        ServerMessage::Waiting { .. } => Kind::Waiting,
    }
}

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
fn every_required_server_message_kind_matches_its_recorded_golden_json() {
    let mut kinds = BTreeSet::new();
    for sample in samples()["server"].as_array().unwrap() {
        // Compare the complete message with committed JSON, including states
        // behind Arc. Comparing an Arc with its own referent is not an oracle.
        kinds.insert(server_kind(&round_trip::<ServerMessage>(sample)));
    }
    let mut required = BTreeSet::new();
    let mut names = BTreeSet::new();
    for &kind in ServerSampleKind::ALL {
        let (name, exclusion) = kind.recording();
        assert!(names.insert(name), "duplicate kind name: {name}");
        if let Some(reason) = exclusion {
            assert!(!reason.is_empty(), "an exclusion needs a reviewed reason");
            assert!(
                !kinds.contains(&kind),
                "{name} is now recorded; remove its obsolete exclusion: {reason}"
            );
        } else {
            required.insert(kind);
        }
    }
    assert_eq!(kinds, required, "recorded server kind coverage changed");
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
fn known_weapon_views_validate_full_source_and_keep_identification_boundary() {
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
        let Some(index) = state.observation.interactions.as_ref().and_then(|view| {
            view.inventory.iter().position(|item| {
                item.known_equipment
                    .as_ref()
                    .is_some_and(|gear| gear.attack.is_some())
            })
        }) else {
            continue;
        };
        let full: AttackView = serde_json::from_value(serde_json::json!({
            "skill":{"id":"light_weaponry","name":"Light Weaponry"}, "bonus":2, "wind_up":90, "recovery":70,
            "damage":{"primary":{"category":"energy","descriptor":{"id":"fire","name":"Fire"},"sides":6},
                "components":[{"category":"energy","descriptor":{"id":"fire","name":"Fire"},"amount":{
                    "type":"rolled","count":2,"sides":6,"bonus":-1}},
                    {"category":"keen","descriptor":null,"amount":{"type":"fixed","value":3}}]}
        }))
        .unwrap();
        state.observation.interactions.as_mut().unwrap().inventory[index]
            .known_equipment
            .as_mut()
            .unwrap()
            .attack = Some(full.clone());
        state.validate().unwrap();
        round_trip::<StateView>(&serde_json::to_value(&state).unwrap());
        checked += 1;
        for fault in 0..5 {
            let mut invalid = state.clone();
            let attack = invalid.observation.interactions.as_mut().unwrap().inventory[index]
                .known_equipment
                .as_mut()
                .unwrap()
                .attack
                .as_mut()
                .unwrap();
            match fault {
                0 => attack.skill.id = "invalid skill id".into(),
                1 => attack.wind_up = 0,
                2 => attack.damage.primary.sides = None,
                3 => attack.damage.components.reverse(),
                _ => {
                    attack.damage.components[0].amount = DamageAmountView::Rolled {
                        count: 65,
                        sides: 6,
                        bonus: -1,
                    }
                }
            }
            assert_eq!(invalid.validate(), Err(InvalidState::InvalidInteraction));
        }
        let mut hidden = state.clone();
        let target = hidden.observation.interactions.as_ref().unwrap().inventory[index].item;
        hidden
            .observation
            .inventory
            .iter_mut()
            .find(|item| item.id == target)
            .unwrap()
            .identified = false;
        assert_eq!(hidden.validate(), Err(InvalidState::InvalidInteraction));
        let mut forged = serde_json::to_value(&full).unwrap();
        forged["damage"]["components"][0]["amount"]["cached_damage"] = 9.into();
        assert!(serde_json::from_value::<AttackView>(forged).is_err());
    }
    assert!(
        checked > 0,
        "recorded public views must include a known weapon"
    );
}
