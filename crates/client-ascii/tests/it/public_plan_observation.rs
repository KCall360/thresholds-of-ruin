#[test]
fn public_plan_ascii_messages_keep_historical_subject_after_occlusion_without_repetition() {
    use tor_client_ascii::messages::Messages;
    use tor_client_common::{narration, test_support};
    use tor_protocol::*;
    let before = test_support::observation();
    let mut after = before.clone();
    after.tick = 100;
    after.combat.as_mut().unwrap().public_plan = Some(PublicPlanObservationView {
        observed_at: 100,
        subject_name: "remembered guard".into(),
        state: PublicPlanStateView::NoPublicPlan,
    });
    let mut messages = Messages::default();
    messages.absorb(&narration::observation_lines(&before, &after, None), &after);
    let expected = "Observed at tick 100: remembered guard had no public plan available.";
    assert!(messages.text().contains(expected));
    let mut later = after.clone();
    later.tick = 500;
    later.visible_actors.clear();
    messages.absorb(&narration::observation_lines(&after, &later, None), &later);
    assert_eq!(messages.text().matches(expected).count(), 1);
}
