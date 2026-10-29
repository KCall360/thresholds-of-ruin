use tor_client_common::{test_support, Palette};
use tor_client_text::engine::{
    chronicle::Chronicler,
    narrate::{compose, Entry, Record},
};
use tor_protocol::*;

#[test]
fn ordered_strikes_text_chronicle_identifies_later_visible_miss_without_opponent_total() {
    let before = StateView {
        snapshot: tor_protocol::SnapshotId("fixture-snapshot".into()),
        wizard_game: false,
        revision: 0,
        observation: test_support::observation(),
    };
    let mut after = before.clone();
    after.revision = 1;
    after.snapshot = tor_protocol::SnapshotId(format!("fixture-display-{}", after.revision));
    after.observation.tick = 20;
    after.observation.combat.as_mut().unwrap().events =
        vec![serde_json::from_value(serde_json::json!({
            "type":"ability","caster":null,"target":after.observation.self_target,
            "ability":{"id":"triple_strike","name":"Triple Strike"},"outcome":"miss","strike":1
        }))
        .unwrap()];
    let beats = Chronicler::default().outcome(&OutcomeFacts::capture(
        &before.observation,
        &after.observation,
        None,
    ));
    let text = compose(&Record {
        entries: vec![Entry::Beats(beats)],
    });
    assert!(text.contains("Triple Strike (strike 2)"), "{text}");
    assert!(text.contains("misses"), "{text}");
    assert!(!text.contains("of 3"), "{text}");
}

#[test]
fn ordered_strikes_text_look_shows_paused_prefix_and_exact_cancel_debt() {
    let mut state = StateView {
        snapshot: tor_protocol::SnapshotId("fixture-snapshot".into()),
        wizard_game: false,
        revision: 0,
        observation: test_support::observation(),
    };
    let mut value = serde_json::to_value(state.observation.combat.as_ref().unwrap()).unwrap();
    value["preparation_remaining"] = serde_json::json!("5");
    value["preparation_active"] = serde_json::json!(false);
    value["sequence"] = serde_json::json!({"completed":1,"total":3,
        "next_strike_remaining":"5","cancel_recovery_remaining":"35"});
    state.observation.combat = Some(serde_json::from_value(value).unwrap());
    let text =
        tor_client_text::adventure::describe_in(&state, &Palette::default(), &Default::default());
    assert!(
        text.contains("Sequence paused after strike 1 of 3"),
        "{text}"
    );
    assert!(text.contains("35 ticks of recovery"), "{text}");
}

#[test]
fn ordered_strikes_text_parses_explicit_cancel_and_step_without_forged_identity() {
    let state = StateView {
        snapshot: tor_protocol::SnapshotId("fixture-snapshot".into()),
        wizard_game: false,
        revision: 0,
        observation: test_support::observation(),
    };
    for line in ["resume", "cancel", "cancel west", "escape north-east"] {
        assert!(tor_client_text::parse(line, &state).is_ok(), "{line}");
    }
    for line in ["cancel sequence west", "escape", "escape nowhere"] {
        assert!(tor_client_text::parse(line, &state).is_err(), "{line}");
    }
}

#[test]
fn ordered_strikes_text_parses_source_selection_and_bounded_page_navigation() {
    let state = StateView {
        snapshot: tor_protocol::SnapshotId("fixture-snapshot".into()),
        wizard_game: true,
        revision: 0,
        observation: test_support::observation(),
    };
    for line in [
        "strikes natural",
        "strikes selected",
        "strikes ability basic_melee",
        "strikes next",
        "strikes previous",
    ] {
        assert!(
            tor_client_text::parse(line, &state).is_ok(),
            "recognized private detail command: {line}"
        );
    }
    for line in [
        "strikes",
        "strikes ability",
        "strikes ability unknown extra",
        "strikes next extra",
    ] {
        assert!(
            tor_client_text::parse(line, &state).is_err(),
            "malformed detail command: {line}"
        );
    }
}
