use tor_client_3d_model::{request_from, Cmd};
use tor_protocol::Request;

#[test]
fn a_command_keeps_the_disclosed_input_boundary_instead_of_acquiring_current_authority() {
    let state = super::projection::state();
    let current = state.reply_context();
    assert!(matches!(
        request_from(&state, &current, Cmd::Control),
        Ok(Request::AcquireControl)
    ));
    let mut cases = Vec::new();
    let mut value = current.clone();
    value.input.stream.epoch += 1;
    cases.push(("stream", value));
    let mut value = current.clone();
    value.input.readiness_revision += 1;
    cases.push(("readiness", value));
    let mut value = current.clone();
    value.branch.0 = "abandoned".into();
    cases.push(("branch", value));
    let mut value = current.clone();
    value.revision += 1;
    cases.push(("revision", value));
    let mut value = current.clone();
    value.cursor.sequence += 1;
    cases.push(("cursor", value));
    let mut value = current.clone();
    value.actor.0 += 1;
    cases.push(("actor", value));
    let accepted: Vec<_> = cases
        .into_iter()
        .filter_map(|(name, shown)| {
            request_from(&state, &shown, Cmd::Control)
                .is_ok()
                .then_some(name)
        })
        .collect();
    assert!(
        accepted.is_empty(),
        "stale boundaries were silently restamped: {accepted:?}"
    );
}
