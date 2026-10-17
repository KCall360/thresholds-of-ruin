use tor_client_3d_model::SceneFrame;
use tor_protocol::{BranchId, CellView, Position, StreamContext, StreamId};

fn cell(key: &str, x: i32) -> CellView {
    CellView {
        lit: true,
        key: key.into(),
        position: Position { x, y: 0, z: 0 },
        wall: false,
        material: String::new(),
        door: None,
        asset: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    }
}

fn context(epoch: u64) -> StreamContext {
    StreamContext {
        stream: StreamId("client".into()),
        epoch,
    }
}

#[test]
fn shared_unique_cells_keep_the_world_still_during_a_step() {
    let mut scene = SceneFrame::default();
    let branch = BranchId("play".into());
    scene.observe(&[cell("a", 0), cell("b", 1)], &context(0), &branch);
    let change = scene.observe(&[cell("a", -1), cell("b", 0)], &context(0), &branch);
    assert!(!change.discontinuous);
    assert_eq!(change.offset, Position { x: 1, y: 0, z: 0 });
}

#[test]
fn unrelated_and_conflicting_charts_cannot_reuse_the_old_translation() {
    for cells in [
        vec![cell("elsewhere", 0)],
        vec![cell("a", -2), cell("b", 5)],
    ] {
        let mut scene = SceneFrame::default();
        let branch = BranchId("play".into());
        scene.observe(&[cell("a", 0), cell("b", 1)], &context(0), &branch);
        scene.observe(&[cell("a", -1), cell("b", 0)], &context(0), &branch);
        let change = scene.observe(&cells, &context(0), &branch);
        assert!(
            change.discontinuous,
            "a camera must snap across an unanchored chart"
        );
        assert_eq!(change.offset, Position { x: 0, y: 0, z: 0 });
    }
}

#[test]
fn branch_or_stream_reset_cannot_retain_presentation_identity() {
    for (next_context, next_branch) in [(context(1), "play"), (context(0), "rewound")] {
        let mut scene = SceneFrame::default();
        scene.observe(&[cell("a", 0)], &context(0), &BranchId("play".into()));
        let change = scene.observe(
            &[cell("a", -1)],
            &next_context,
            &BranchId(next_branch.into()),
        );
        assert!(
            change.discontinuous,
            "old actors, effects and corpses must reset"
        );
    }
}

#[test]
fn repeated_cell_identity_is_not_a_translation_anchor() {
    let mut scene = SceneFrame::default();
    let branch = BranchId("play".into());
    scene.observe(&[cell("a", 0)], &context(0), &branch);
    let change = scene.observe(&[cell("a", -1), cell("a", 3)], &context(0), &branch);
    assert!(change.discontinuous);
}

#[test]
fn translation_outside_the_render_grid_resets_without_wrapping() {
    let mut scene = SceneFrame::default();
    let branch = BranchId("play".into());
    scene.observe(&[cell("a", i32::MIN)], &context(0), &branch);
    let change = scene.observe(&[cell("a", i32::MAX)], &context(0), &branch);
    assert!(change.discontinuous);
    assert_eq!(change.offset, Position { x: 0, y: 0, z: 0 });
}
