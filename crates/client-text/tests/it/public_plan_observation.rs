use tor_client_common::{test_support, Palette};
use tor_client_text::engine::{
    chronicle::Chronicler,
    narrate::{compose, Entry, Record},
};
use tor_protocol::*;
fn state() -> StateView {
    StateView {
        snapshot: tor_protocol::SnapshotId("fixture-snapshot".into()),
        wizard_game: false,
        revision: 0,
        observation: test_support::observation(),
    }
}
fn observed(mut state: StateView) -> StateView {
    state.observation.tick = 100;
    state.observation.combat.as_mut().unwrap().public_plan = Some(PublicPlanObservationView {
        observed_at: 100,
        subject_name: "remembered guard".into(),
        state: PublicPlanStateView::Preparing {
            ability: None,
            aim: ObservedAimView::Unavailable,
            remaining_work: 200,
        },
    });
    state
}
const LINE: &str = "Observed at tick 100: remembered guard was preparing; ability undisclosed; aim unavailable; 200 work remaining.";
#[test]
fn public_plan_text_chronicle_reports_new_paid_history_once_without_target_lookup() {
    let before = state();
    let after = observed(before.clone());
    let mut chronicler = Chronicler::default();
    let beats = chronicler.outcome(
        &OutcomeFacts::capture(&before.observation, &after.observation, None),
        &tor_client_common::Palette::default(),
    );
    let text = compose(&Record {
        entries: vec![Entry::Beats(beats)],
    });
    assert!(text.contains(LINE), "{text}");
    let mut later = after.clone();
    later.observation.tick = 500;
    later.observation.visible_actors.clear();
    let beats = chronicler.outcome(
        &OutcomeFacts::capture(&after.observation, &later.observation, None),
        &tor_client_common::Palette::default(),
    );
    let text = compose(&Record {
        entries: vec![Entry::Beats(beats)],
    });
    assert!(
        !text.contains("Observed at tick"),
        "a later clock/occlusion never recaptures history"
    );
}
#[test]
fn public_plan_text_snapshot_descriptions_repeat_the_exact_past_record() {
    let empty = state();
    assert!(!tor_client_text::describe(&empty).contains("Observed at tick"));
    let mut state = observed(empty);
    state.observation.tick = 500;
    state.observation.visible_actors.clear();
    for text in [
        tor_client_text::describe(&state),
        tor_client_text::adventure::describe_in(&state, &Palette::default(), &Default::default()),
    ] {
        assert!(text.contains(LINE), "{text}");
        assert!(!text.contains("slow_plan"));
    }
}
