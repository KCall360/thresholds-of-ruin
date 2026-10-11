//! Synthetic broken traces prove that the shared workload oracle fails closed.
use tor_protocol::{Action, Direction, DoorTarget, DoorView, StateView};
use tor_test_support::performance::{Step, Trace, TraceAction};

fn view() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":"4","observation":{
            "actor":"1","self_target":tor_protocol::ActorTarget::from_digest([1;32]),
            "tick":"5","position":{"x":0,"y":0,"z":0},"places":[],
            "visible_cells":[
                {"key":"origin","position":{"x":0,"y":0,"z":0},"wall":false,"stairs_up":false,"stairs_down":false,"place_hint":false},
                {"key":"east","position":{"x":1,"y":0,"z":0},"wall":false,"stairs_up":false,"stairs_down":false,"place_hint":false}
            ],"ground_items":[],"inventory":[],"visible_actors":[],"ready":true
        }
    })).unwrap()
}
fn step(action: TraceAction, expected: &str) -> Step {
    Step {
        label: "synthetic guard".into(),
        action,
        expected: expected.into(),
        min_regions: 1,
    }
}
fn door(id: u8) -> DoorView {
    DoorView {
        id: DoorTarget::from_digest([id; 32]),
        name: "door".into(),
        description: "door".into(),
        open: false,
        reachable: true,
        approaches: vec!["origin".into()],
        asset: None,
    }
}
fn door_step() -> Step {
    step(TraceAction::Door { open: true }, "door_changed")
}

#[test]
#[should_panic(expected = "unexpectedly accepted")]
fn blocked_trace_rejects_an_accepted_action() {
    step(TraceAction::Wait, "blocked").verify(&view(), &view(), true);
}
#[test]
#[should_panic(expected = "advanced state")]
fn blocked_trace_rejects_any_mutated_state_even_without_a_revision_change() {
    let before = view();
    let mut after = before.clone();
    after.wizard_game = true;
    step(TraceAction::Wait, "blocked").verify(&before, &after, false);
}
#[test]
#[should_panic(expected = "unexpectedly rejected")]
fn successful_trace_rejects_a_rejected_action() {
    step(TraceAction::Wait, "waited").verify(&view(), &view(), false);
}
#[test]
#[should_panic(expected = "failed to advance")]
fn successful_trace_requires_revision_progress() {
    step(TraceAction::Wait, "waited").verify(&view(), &view(), true);
}
#[test]
#[should_panic(expected = "disclosed destination offset")]
fn movement_trace_rejects_a_different_disclosed_destination_key() {
    let before = view();
    let mut after = before.clone();
    after.revision += 1;
    after.observation.visible_cells[0].key = "wrong_destination".into();
    step(
        TraceAction::Move {
            direction: Direction::East,
        },
        "moved",
    )
    .verify(&before, &after, true);
}
#[test]
#[should_panic(expected = "expected one disclosed reachable door")]
fn door_resolution_rejects_no_matching_door() {
    let mut before = view();
    let mut disclosed = door(1);
    disclosed.reachable = false;
    before.observation.visible_cells[0].door = Some(disclosed);
    door_step().resolve(&before);
}
#[test]
#[should_panic(expected = "expected one disclosed reachable door")]
fn door_resolution_rejects_two_distinct_eligible_doors() {
    let mut before = view();
    before.observation.visible_cells[0].door = Some(door(1));
    before.observation.visible_cells[1].door = Some(door(2));
    door_step().resolve(&before);
}
#[test]
fn duplicate_views_of_one_door_resolve_and_verify_successfully() {
    let mut before = view();
    for cell in &mut before.observation.visible_cells {
        cell.door = Some(door(1));
    }
    let action = door_step().resolve(&before);
    assert_eq!(
        action,
        Action::SetDoor {
            door: DoorTarget::from_digest([1; 32]),
            open: true
        }
    );
    let mut after = before.clone();
    after.revision += 1;
    for cell in &mut after.observation.visible_cells {
        cell.door.as_mut().unwrap().open = true;
    }
    door_step().verify(&before, &after, true);
}
#[test]
fn truthful_blocked_wait_and_disclosed_move_traces_pass() {
    let before = view();
    step(TraceAction::Wait, "blocked").verify(&before, &before, false);
    let mut after = before.clone();
    after.revision += 1;
    step(TraceAction::Wait, "waited").verify(&before, &after, true);
    after.observation.visible_cells[0].key = "east".into();
    step(
        TraceAction::Move {
            direction: Direction::East,
        },
        "moved",
    )
    .verify(&before, &after, true);
}
#[test]
fn versioned_trace_accepts_only_known_expected_outcomes() {
    let fixture: serde_json::Value =
        serde_json::from_str(tor_test_support::performance::SPEC).unwrap();
    assert_eq!(Trace::load().version, 1);
    for expected in ["blocked", "waited", "moved", "door_changed"] {
        let value =
            serde_json::json!({"label":"guard", "action":{"type":"wait"},"expected":expected});
        assert!(serde_json::from_value::<Step>(value).is_ok());
    }
    let mut invalid = fixture;
    invalid["steps"][0]["expected"] = serde_json::json!("waied");
    assert!(
        serde_json::from_value::<Trace>(invalid).is_err(),
        "misspelled expectations must not silently take the success path"
    );
}

#[test]
#[should_panic(expected = "unknown expected outcome")]
fn manually_constructed_steps_cannot_bypass_expected_outcome_validation() {
    let before = view();
    let mut after = before.clone();
    after.revision += 1;
    step(TraceAction::Wait, "waied").verify(&before, &after, true);
}

#[test]
#[should_panic(expected = "actor not ready")]
fn trace_resolution_requires_a_ready_actor() {
    let mut before = view();
    before.observation.ready = false;
    step(TraceAction::Wait, "waited").resolve(&before);
}
