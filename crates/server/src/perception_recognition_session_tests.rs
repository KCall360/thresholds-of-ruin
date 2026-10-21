//! Encoded authority moves from apparent-item to creature for one observer only.
use super::*;

#[test]
fn paid_recognition_retires_only_the_knowing_observers_apparent_handle() {
    // Select a reproducible ordinary first-sight failure, rather than editing
    // guise or observer receipts. Paid Search's fixture-only bonus guarantees
    // the later transition while production X/MI1/MI2 remain unchanged.
    let mut wire = Wire::unrecognized_xeroc();
    let before = wire.snapshot(OWNER);
    let witness_before = wire.snapshot(WITNESS);
    let apparent = before
        .state
        .observation
        .ground_items
        .iter()
        .find(|row| {
            // Wire positions are observer-relative: X(3,2) - owner(2,1).
            row.position.x == 1
                && row.position.y == 1
                && row.position.z == 0
                && row.item.asset.as_deref() == Some("item.weapon.mace")
        })
        .expect("actual disclosed apparent mace")
        .item
        .id;
    let self_target = before.state.observation.self_target;
    let subject_before = wire.service.engine.observation(SUBJECT).unwrap();
    wire.service.engine.assert_perception_subject_idle(SUBJECT);
    let (search, _) = wire.act(
        OWNER,
        "recognize-guise",
        attack("rogue.search", self_target),
    );
    wire.finish("recognize-guise", &search);
    let after = wire.snapshot(OWNER);
    let creature = after
        .state
        .observation
        .visible_actors
        .iter()
        .find(|row| row.name == "xeroc")
        .expect("successful private recognition")
        .id;
    assert!(!after
        .state
        .observation
        .ground_items
        .iter()
        .any(|row| row.item.id == apparent));
    let witness_after = wire.snapshot(WITNESS);
    assert!(!witness_after
        .state
        .observation
        .visible_actors
        .iter()
        .any(|row| row.name == "xeroc"));
    assert_eq!(
        witness_after.state.observation.ground_items,
        witness_before.state.observation.ground_items
    );
    let subject_after = wire.service.engine.observation(SUBJECT).unwrap();
    assert_eq!(subject_after.inventory, subject_before.inventory);
    // Interaction offers are absent for this external subject; their absence
    // is not evidence of idleness. Read actual preparation through a test-only
    // Engine-owned assertion, with the same assertion established before Search.
    wire.service.engine.assert_perception_subject_idle(SUBJECT);
    wire.reject(
        "retired-apparent",
        Action::Take {
            item: apparent,
            quantity: None,
        },
        ErrorCode::InvalidAction,
    );
    // The newly disclosed actor reference now licenses ordinary sight targeting.
    let (probe, _) = wire.act(OWNER, "recognized-sight", attack("fixture_sight", creature));
    wire.finish("recognized-sight", &probe);
    assert!(!wire
        .snapshot(WITNESS)
        .state
        .observation
        .visible_actors
        .iter()
        .any(|row| row.name == "xeroc"));
}
