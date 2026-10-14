// New client-common integration module. Only existing public stats/narration APIs.
use tor_protocol::{CombatView, ObservedAimView, PublicPlanObservationView, PublicPlanStateView};
fn combat() -> CombatView {
    serde_json::from_value(serde_json::json!({"hp":10,"max_hp":10,"preparation_remaining":null,"preparation_active":false,
        "sequence":null,"public_plan":null,"recovery_remaining":"0","actors":[],"events":[],"objective":null,
        "victory":false,"dead":false,"terminal":false})).unwrap()
}
fn record() -> PublicPlanObservationView {
    PublicPlanObservationView {
        observed_at: 100,
        subject_name: "guard".into(),
        state: PublicPlanStateView::Preparing {
            ability: None,
            aim: ObservedAimView::Unavailable,
            remaining_work: 200,
        },
    }
}
#[test]
fn public_plan_stats_present_one_explicitly_historical_record_without_own_stats() {
    let mut view = combat();
    assert!(!tor_client_common::stats::lines(Some(&view))
        .iter()
        .any(|line| line.starts_with("Observed at tick")));
    view.public_plan = Some(record());
    let rows = tor_client_common::stats::lines(Some(&view));
    assert_eq!(
        rows.iter()
            .filter(|row| row.starts_with("Observed at tick"))
            .count(),
        1
    );
    assert!(rows.iter().any(|row| row == "Observed at tick 100: guard was preparing; ability undisclosed; aim unavailable; 200 work remaining."));
    view.public_plan.as_mut().unwrap().state = PublicPlanStateView::NoPublicPlan;
    assert!(tor_client_common::stats::lines(Some(&view))
        .iter()
        .any(|row| row == "Observed at tick 100: guard had no public plan available."));
}
#[test]
fn public_plan_stats_use_only_frozen_offsets_and_never_offer_a_target_handle() {
    let mut view = combat();
    let mut record = record();
    record.state = PublicPlanStateView::Preparing {
        ability: Some(tor_protocol::ContentRef {
            id: "slow_plan".into(),
            name: "Slow Plan".into(),
        }),
        aim: ObservedAimView::Disclosed {
            cell: "opaque-old-key".into(),
            occurrences: vec![tor_protocol::Position { x: 1, y: 2, z: 0 }],
        },
        remaining_work: 200,
    };
    view.public_plan = Some(record);
    let text = tor_client_common::stats::lines(Some(&view)).join("\n");
    assert!(text.contains(
        "was preparing Slow Plan; aim (1,2,0) in your frame at that time; 200 work remaining"
    ));
    assert!(!text.contains("opaque-old-key"));
    assert!(!text.contains("slow_plan"));
}
#[test]
fn public_plan_narration_announces_new_paid_records_once_and_preserves_occluded_history() {
    let before = tor_client_common::test_support::observation();
    let mut after = before.clone();
    after.tick = 100;
    after.combat.as_mut().unwrap().public_plan = Some(record());
    let lines = tor_client_common::narration::observation(&before, &after, None);
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("Observed at tick"))
            .count(),
        1
    );
    assert!(lines.iter().any(|line| line == "Observed at tick 100: guard was preparing; ability undisclosed; aim unavailable; 200 work remaining."));
    let mut later = after.clone();
    later.tick = 500;
    later.visible_actors.clear();
    assert!(
        tor_client_common::narration::observation(&after, &later, None)
            .iter()
            .all(|line| !line.starts_with("Observed at tick"))
    );
    assert_eq!(
        tor_client_common::stats::lines(after.combat.as_ref()),
        tor_client_common::stats::lines(later.combat.as_ref())
    );
    assert!(tor_client_common::narration::observation(&later, &later, None).is_empty());
    let mut paid_again = later.clone();
    paid_again
        .combat
        .as_mut()
        .unwrap()
        .public_plan
        .as_mut()
        .unwrap()
        .observed_at = 500;
    assert_eq!(
        tor_client_common::narration::observation(&later, &paid_again, None)
            .iter()
            .filter(|line| line.starts_with("Observed at tick 500"))
            .count(),
        1
    );
}
