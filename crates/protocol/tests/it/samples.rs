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
        Request::Map { .. } => "map",
        Request::Command { command, .. } => match command {
            Command::Advancement { .. } => "command.advancement",
            Command::InspectStrikeSource { .. } => "command.inspect_strike_source",
            Command::ResumeIntention { .. } => "command.resume_intention",
            Command::CancelIntention { .. } => "command.cancel_intention",
            Command::RenamePlace { .. } => "command.rename_place",
            Command::Travel { .. } => "command.travel",
            Command::Wizard { .. } => "command.wizard",
            Command::Annotate { .. } => "command.annotate",
            Command::Act { action, .. } => match action {
                Action::Escape { .. } => "act.escape",
                Action::ChooseGrowthSource { .. } => "act.choose_growth_source",
                Action::Purchase { .. } => "act.purchase",
                Action::Respec { .. } => "act.respec",
                Action::UseAbility { .. } => "act.use_ability",
                Action::Equip { .. } => "act.equip",
                Action::Unequip { .. } => "act.unequip",
                Action::Drink { .. } => "act.drink",
                Action::Read { .. } => "act.read",
                Action::Zap { .. } => "act.zap",
                Action::Identify { .. } => "act.identify",
                Action::Eat { .. } => "act.eat",
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
    "act.escape",
    "hello",
    "continue",
    "save",
    "history_branch",
    "attach",
    "acquire_control",
    "release_control",
    "snapshot",
    "palette",
    "map",
    "command.rename_place",
    "command.resume_intention",
    "command.cancel_intention",
    "command.inspect_strike_source",
    "command.advancement",
    "command.travel",
    "command.wizard",
    "command.annotate",
    "act.equip",
    "act.unequip",
    "act.drink",
    "act.read",
    "act.zap",
    "act.identify",
    "act.eat",
    "act.use_ability",
    "act.choose_growth_source",
    "act.purchase",
    "act.respec",
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
    CreatureCatalog => ("creature_catalog", None),
    CreatureInspection => ("creature_inspection", None),
    CreatureStrikePage => ("creature_strike_page", None),
    AdvancementPage => ("advancement_page", None),
    MapPage => ("map_page", None),
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
        ServerMessage::CreatureCatalog { .. } => Kind::CreatureCatalog,
        ServerMessage::CreatureInspection { .. } => Kind::CreatureInspection,
        ServerMessage::CreatureStrikePage { .. } => Kind::CreatureStrikePage,
        ServerMessage::AdvancementPage { .. } => Kind::AdvancementPage,
        ServerMessage::MapPage { .. } => Kind::MapPage,
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
fn recorded_request_envelopes_preserve_payload_bytes_and_validation() {
    let mut checked = 0;
    for sample in samples()["client"].as_array().unwrap() {
        if sample["type"] != "request" {
            continue;
        }
        let message = round_trip::<ClientMessage>(sample);
        let payload: Request = serde_json::from_value(sample["request"].clone()).unwrap();
        assert_eq!(
            serde_json::to_string(&message).unwrap(),
            format!(
                r#"{{"type":"request","request_id":{},"request":{}}}"#,
                serde_json::to_string(&sample["request_id"]).unwrap(),
                serde_json::to_string(&payload).unwrap()
            )
        );
        let mut extra_envelope_field = sample.clone();
        extra_envelope_field["extra"] = Value::Bool(true);
        assert!(serde_json::from_value::<ClientMessage>(extra_envelope_field).is_err());
        let mut extra_request_field = sample.clone();
        extra_request_field["request"]["extra"] = Value::Bool(true);
        // The envelope must preserve the payload's existing schema, including
        // Serde's distinct handling of unit and struct request variants.
        let payload_accepts_extra =
            serde_json::from_value::<Request>(extra_request_field["request"].clone()).is_ok();
        if matches!(
            payload,
            Request::Attach { .. }
                | Request::Map { .. }
                | Request::Command { .. }
                | Request::History { .. }
                | Request::HistoryBranch { .. }
        ) {
            assert!(
                !payload_accepts_extra,
                "struct requests reject extra fields"
            );
        }
        assert_eq!(
            serde_json::from_value::<ClientMessage>(extra_request_field).is_ok(),
            payload_accepts_extra
        );
        checked += 1;
    }
    assert!(checked > 0, "no recorded request envelopes were checked");
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
fn recorded_sequence_rider_inspection_contains_explicit_null_and_typed_gate() {
    let mut ordinary = false;
    let mut conditional = false;
    for sample in samples()["server"].as_array().unwrap() {
        let ServerMessage::CreatureStrikePage { page, .. } = round_trip::<ServerMessage>(sample)
        else {
            continue;
        };
        page.validate().unwrap();
        for entry in &page.entries {
            if let StrikePageEntry::RiderHeader {
                trigger, condition, ..
            } = entry
            {
                match condition {
                    None => ordinary = true,
                    Some(InspectionRiderCondition::SequenceSuccessfulHits { minimum: 2 }) => {
                        assert_eq!(*trigger, RiderTrigger::Success);
                        conditional = true;
                    }
                    Some(_) => {}
                }
            }
        }
    }
    assert!(ordinary, "record an ordinary explicit-null rider");
    assert!(
        conditional,
        "record the actual server's non-null sequence gate with scripts/record_wire_samples.py"
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

#[test]
fn recorded_cancel_commands_require_an_explicit_step() {
    let value = samples();
    let cancel = value["client"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["request"]["command"]["type"] == "cancel_intention")
        .unwrap();
    round_trip::<ClientMessage>(cancel);
    let mut missing = cancel.clone();
    assert!(missing["request"]["command"]
        .as_object_mut()
        .unwrap()
        .remove("step")
        .is_some());
    assert!(serde_json::from_value::<ClientMessage>(missing).is_err());
}

#[test]
fn recorded_ability_commands_require_explicit_invocations_and_supplies() {
    let value = samples();
    let message = value["client"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| {
            let action = &message["request"]["command"]["action"];
            action["type"] == "use_ability" && action["invocation"]["kind"] == "actor_target"
        })
        .unwrap();
    round_trip::<ClientMessage>(message);
    for field in ["invocation", "supplies"] {
        let mut missing = message.clone();
        assert!(missing["request"]["command"]["action"]
            .as_object_mut()
            .unwrap()
            .remove(field)
            .is_some());
        assert!(
            serde_json::from_value::<ClientMessage>(missing).is_err(),
            "accepted missing {field}"
        );
    }
    for keep_invocation in [false, true] {
        let mut implicit = message.clone();
        let action = &mut implicit["request"]["command"]["action"];
        action["target"] = action["invocation"]["target"].clone();
        if !keep_invocation {
            action.as_object_mut().unwrap().remove("invocation");
        }
        assert!(
            serde_json::from_value::<ClientMessage>(implicit).is_err(),
            "target must exist only inside the explicit invocation"
        );
    }
}

#[test]
fn playable_rogue_samples_pin_form_choices_and_both_explicit_invocations() {
    let value = samples();
    let actions: Vec<_> = value["client"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| &message["request"]["command"]["action"])
        .collect();
    for kind in ["actor_target", "self_step"] {
        assert!(actions.iter().any(|action| {
            action["type"] == "use_ability" && action["invocation"]["kind"] == kind
        }));
    }
    for action in actions
        .iter()
        .filter(|action| action["type"] == "use_ability")
    {
        assert!(action.get("target").is_none(), "no legacy target alias");
        assert_eq!(action.as_object().unwrap().len(), 4);
        assert_eq!(action["invocation"].as_object().unwrap().len(), 2);
    }
    assert!(actions.iter().any(|action| {
        action["type"] == "purchase"
            && action["purchase"]["selection"]
                == serde_json::json!({"kind":"talent_form", "talent":"evocation_initiate", "form":"spark_lance_cold"})
    }));
}

#[test]
fn ordered_strike_current_samples_include_both_cancel_forms_and_actual_sequence() {
    let value = samples();
    let cancels: Vec<_> = value["client"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["request"]["command"]["type"] == "cancel_intention")
        .map(|message| &message["request"]["command"]["step"])
        .collect();
    assert!(cancels.contains(&&serde_json::Value::Null));
    assert!(cancels.contains(&&serde_json::json!("west")));
    assert!(value["server"].as_array().unwrap().iter().any(|message| {
        message["snapshot"]["state"]["observation"]["combat"]["sequence"]["completed"] == 1
    }));
}

#[test]
fn phase5_recorded_client_actions_include_both_supply_selection_forms() {
    let value = samples();
    let actions: Vec<_> = value["client"]
        .as_array()
        .unwrap()
        .iter()
        .map(|message| &message["request"]["command"]["action"])
        .filter(|action| action["type"] == "use_ability")
        .collect();
    assert!(actions
        .iter()
        .all(|action| action.get("supplies").is_some()));
    assert!(actions
        .iter()
        .any(|action| action["supplies"]["kind"] == "carried"));
    assert!(actions.iter().any(|action| {
        action["id"] == "bind_wounds"
            && action["supplies"]["kind"] == "explicit"
            && action["supplies"]["items"][0]["cost_ordinal"] == 0
            && action["supplies"]["items"][0]["item"]
                .as_str()
                .is_some_and(|item| item.starts_with("i_"))
    }));
}

#[test]
fn phase5_recorded_care_snapshots_show_physical_stock_and_cooldown_without_claims() {
    fn private_claim_key(value: &Value) -> bool {
        match value {
            Value::Object(fields) => fields.iter().any(|(key, value)| {
                matches!(
                    key.as_str(),
                    "holds" | "item_payment" | "item_claims" | "claimant"
                ) || private_claim_key(value)
            }),
            Value::Array(values) => values.iter().any(private_claim_key),
            _ => false,
        }
    }
    let value = samples();
    let mut prepared = false;
    let mut spent = false;
    for message in value["server"].as_array().unwrap() {
        let observation = &message["snapshot"]["state"]["observation"];
        if observation.is_null() {
            continue;
        }
        assert!(!private_claim_key(observation));
        let Some(abilities) = observation["combat"]["own_stats"]["abilities"].as_array() else {
            continue;
        };
        let Some(care) = abilities
            .iter()
            .find(|ability| ability["id"] == "bind_wounds")
        else {
            continue;
        };
        assert_eq!(care["supply_costs"][0]["quantity"], "1");
        let remaining = care["cooldown_remaining"]
            .as_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let has_bandage = observation["ground_items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|row| {
                row["item"]["known_supply"]["id"] == "bandage"
                    && row["item"]["quantity"] == "1"
                    && row["supply_reachable"] == true
            });
        prepared |=
            has_bandage && remaining == 1200 && observation["combat"]["preparation_active"] == true;
        spent |= !has_bandage
            && (1..1200).contains(&remaining)
            && observation["combat"]["preparation_remaining"].is_null();
    }
    assert!(
        prepared,
        "record an actual finite-care preparation snapshot"
    );
    assert!(
        spent,
        "record an actual exact last-bandage completion snapshot"
    );
}

#[test]
fn public_plan_recorded_snapshot_contains_only_paid_frozen_disclosed_fields() {
    let value = samples();
    let snapshots: Vec<_> = value["server"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| {
            message["type"] == "snapshot"
                && !message["snapshot"]["state"]["observation"]["combat"]["public_plan"].is_null()
        })
        .collect();
    assert_eq!(
        snapshots.len(),
        1,
        "the sanctioned recorder captures one actual paid result"
    );
    let message = snapshots[0];
    round_trip::<ServerMessage>(message);
    let record = &message["snapshot"]["state"]["observation"]["combat"]["public_plan"];
    assert_eq!(record["observed_at"], "100");
    assert_eq!(record["subject_name"], "subject");
    assert_eq!(record["state"]["kind"], "preparing");
    assert!(record["state"]["ability"].is_null());
    assert_eq!(record["state"]["remaining_work"], "200");
    assert_eq!(record["state"]["aim"]["kind"], "disclosed");
    assert!(!record["state"]["aim"]["cell"].as_str().unwrap().is_empty());
    assert!(!record["state"]["aim"]["occurrences"]
        .as_array()
        .unwrap()
        .is_empty());
    let encoded = serde_json::to_string(record).unwrap();
    for hidden in [
        "target_actor",
        "location",
        "region",
        "slow_plan",
        "Slow Plan",
    ] {
        assert!(!encoded.contains(hidden), "paid result leaked {hidden}");
    }
}
