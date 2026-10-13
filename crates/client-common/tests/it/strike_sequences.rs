//! Sequence progress is personal disclosure; visible events expose only ordinals.
use std::sync::Arc;
use tor_client_common::{narration, ClientState, StreamError};
use tor_protocol::*;

fn combat(active: bool, completed: u16) -> CombatView {
    serde_json::from_value(serde_json::json!({
        "hp":12,"max_hp":12,"preparation_remaining":"5","preparation_active":active,
        "recovery_remaining":"0","actors":[],"events":[],"objective":null,
        "victory":false,"dead":false,"terminal":false,
        "sequence":{"completed":completed,"total":3,"next_strike_remaining":"5",
                    "cancel_recovery_remaining":if completed == 0 { "0" } else { "35" }}
    }))
    .unwrap()
}

#[test]
fn ordered_strikes_personal_status_distinguishes_prefix_pause_and_exact_debt() {
    let active = narration::combat_status(&combat(true, 1));
    assert!(active.contains("Strike 1 of 3 complete."), "{active}");
    assert!(active.contains("Next strike in 5 ticks."), "{active}");
    assert!(
        active.contains("Cancel leaves 35 ticks of recovery."),
        "{active}"
    );
    assert!(!active.contains("Attack preparation"), "{active}");
    let paused = narration::combat_status(&combat(false, 1));
    assert!(
        paused.contains("Sequence paused after strike 1 of 3."),
        "{paused}"
    );
    assert!(paused.contains("35 ticks of recovery"), "{paused}");
    let first = narration::combat_status(&combat(true, 0));
    assert!(
        first.contains("Attack preparation: 5 ticks remaining"),
        "{first}"
    );
    assert!(!first.contains("Strike 0"), "{first}");
}

#[test]
fn ordered_strikes_narration_preserves_first_wording_and_names_later_misses_only() {
    let before = tor_client_common::test_support::observation();
    for (strike, expected) in [
        (None, "Something used Triple Strike on you, but missed."),
        (Some(0), "Something used Triple Strike on you, but missed."),
        (
            Some(2),
            "Something used Triple Strike (strike 3) on you, but missed.",
        ),
    ] {
        let event: CombatEventView = serde_json::from_value(serde_json::json!({
            "type":"ability","caster":null,"target":before.self_target,
            "ability":{"id":"triple_strike","name":"Triple Strike"},
            "outcome":"miss","strike":strike
        }))
        .unwrap();
        assert_eq!(narration::combat_event(&event, &before, &before), expected);
        assert!(!expected.contains("of 3"), "opponent totals are private");
    }
}

#[test]
fn ordered_strikes_progress_changes_are_narrated_without_hp_damage() {
    let before = tor_client_common::test_support::observation();
    let mut after = before.clone();
    after.tick = 15;
    after.combat = Some(combat(false, 1));
    let lines = narration::observation(&before, &after, None);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("Sequence paused after strike 1 of 3")),
        "{lines:?}"
    );
}

#[test]
fn ordered_strikes_invalid_sequence_delta_keeps_the_entire_client_unchanged() {
    let initial = super::validation::snapshot(0);
    let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
    let mut next = initial.state.as_ref().clone();
    next.revision = 1;
    next.observation.tick = 1;
    next.observation.combat = Some(combat(true, 3));
    let before = client.clone();
    // Build the malformed payload independently: a completed sequence cannot
    // still own preparation. Reject it before replacing any client state.
    let mut delta = StateDelta::between(client.state(), client.state()).unwrap();
    delta.revision = 1;
    delta.tick = 1;
    delta.combat = next.observation.combat;
    let result = client.apply(StreamUpdate {
        context: initial.context,
        actor: initial.actor,
        branch: initial.branch,
        cursor: StreamCursor {
            sequence: 1,
            tick: 1,
        },
        body: UpdateBody::ObservationDelta {
            base: client.observation_base(),
            state: Box::new(delta),
            event: None,
        },
    });
    assert_eq!(result, Err(StreamError::InconsistentState));
    assert_eq!(client, before);
}

#[test]
fn ordered_strikes_in_place_cancel_carries_an_explicit_null_step() {
    let mut snapshot = super::validation::snapshot(0);
    snapshot.has_control = true;
    Arc::make_mut(&mut snapshot.state).observation.combat = Some(combat(false, 1));
    snapshot.intentions = vec![IntentionStatus {
        actor: snapshot.actor,
        branch: snapshot.branch.clone(),
        intention: IntentionId("sequence".into()),
        entry_id: EntryId("admitted".into()),
        phase: IntentionPhase::Paused,
    }];
    snapshot.readiness.cancel = vec![IntentionId("sequence".into())];
    let client = ClientState::from_snapshot(snapshot).unwrap();
    let request = serde_json::to_value(client.cancel_intention_request().unwrap()).unwrap();
    assert!(request["command"].as_object().unwrap().contains_key("step"));
    assert!(request["command"]["step"].is_null());
}

#[test]
fn ordered_strikes_nonterminal_victory_keeps_sequence_status() {
    let mut view = combat(true, 1);
    view.victory = true;
    let text = narration::combat_status(&view);
    assert!(
        text.contains("Victory! You may continue exploring."),
        "{text}"
    );
    assert!(text.contains("Strike 1 of 3 complete."), "{text}");
    assert!(text.contains("Next strike in 5 ticks."), "{text}");
    assert!(
        text.contains("Cancel leaves 35 ticks of recovery."),
        "{text}"
    );
}
