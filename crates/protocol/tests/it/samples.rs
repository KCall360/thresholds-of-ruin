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
    "command.travel",
    "command.wizard",
    "command.annotate",
    "act.equip",
    "act.unequip",
    "act.drink",
    "act.attack",
    "act.use_ability",
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
        ServerMessage::SnapshotPart { .. } => "snapshot_part",
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
        ServerMessage::CreatureInspection { .. } => "creature_inspection",
        ServerMessage::CombatDiagnostics { .. } => "combat_diagnostics",
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
    "snapshot_part",
    "update.travel",
    "update.observation_delta",
    "update.annotation",
    "update.control",
    "ack.immediate",
    "ack.admitted",
    "creature_inspection",
    "combat_diagnostics",
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
            "skill":"light_weaponry", "bonus":2, "wind_up":90, "recovery":70,
            "damage":{"primary":{"category":"energy","descriptor":"fire","sides":6},
                "components":[{"category":"energy","descriptor":"fire","amount":{
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
                0 => attack.skill = Skill::Spellcasting,
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
