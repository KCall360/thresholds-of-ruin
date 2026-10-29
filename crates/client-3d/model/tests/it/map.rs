use tor_client_3d_model::{map_destinations, view, Cmd, MapSelection, View};
use tor_client_common::Palette;
use tor_protocol::*;
fn cell(x: i32, y: i32) -> CellView {
    CellView {
        object_presentation: None,
        lit: true,
        key: format!("opaque:{x}:{y}"),
        position: Position { x, y, z: 0 },
        wall: false,
        material: String::new(),
        trap: None,
        door: None,
        asset: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    }
}
fn fixture() -> View {
    let mut v = view(&super::projection::state(), &Palette::default());
    v.feet = Position { x: 0, y: 0, z: 0 };
    v.has_control = true;
    v.can_act = true;
    v.actors.clear();
    v.items.clear();
    v.memory.clear();
    v.visible = vec![cell(0, 0), cell(2, 1), cell(-1, 0), cell(1, 0)];
    v
}
fn remembered(x: i32, y: i32) -> RememberedMapCell {
    RememberedMapCell {
        object_presentation: None,
        key: format!("memory:{x}:{y}"),
        position: Position { x, y, z: 0 },
        wall: false,
        material: String::new(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        door: None,
        trap: None,
        ground_items: vec![],
    }
}
#[test]
fn destinations_use_exact_disclosed_keys_in_map_reading_order() {
    let v = fixture();
    let rows = map_destinations(&v);
    assert_eq!(
        rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
        ["opaque:-1:0", "opaque:1:0", "opaque:2:1"]
    );
    assert_eq!(rows[2].position, Position { x: 2, y: 1, z: 0 });
    let selection = MapSelection::select(&v, "opaque:1:0").unwrap();
    assert!(matches!(selection.command(&v,true),Some(Cmd::Travel(key)) if key=="opaque:1:0"));
    assert!(MapSelection::select(&v, "unknown").is_none());
}
#[test]
fn walls_closed_doors_traps_and_other_levels_are_not_route_shortcuts() {
    let mut v = fixture();
    v.visible[1].wall = true;
    v.visible[2].door = Some(DoorView {
        id: DoorTarget::from_digest([2; 32]),
        name: "door".into(),
        description: String::new(),
        open: false,
        reachable: false,
        asset: None,
        approaches: vec![],
    });
    v.visible[3].position.z = 1;
    let mut trap = remembered(3, 0);
    trap.trap = Some(MapTrapAppearance { family: None });
    v.memory.push(trap);
    assert!(map_destinations(&v).is_empty());
}
#[test]
fn visible_cells_override_stale_memory_and_duplicates_are_rejected() {
    let mut v = fixture();
    v.memory = vec![remembered(1, 0), remembered(4, 0)];
    let keys: Vec<_> = map_destinations(&v).into_iter().map(|r| r.key).collect();
    assert_eq!(
        keys,
        ["opaque:-1:0", "opaque:1:0", "memory:4:0", "opaque:2:1"]
    );
    v.visible.push(v.visible[3].clone());
    assert!(MapSelection::select(&v, "opaque:1:0").is_none());
    v.visible.pop();
    v.visible[1].key = v.visible[3].key.clone();
    assert!(MapSelection::select(&v, "opaque:1:0").is_none());
}
#[test]
fn selection_is_read_only_and_requires_control_readiness_and_current_geometry() {
    let v = fixture();
    let s = MapSelection::select(&v, "opaque:1:0").unwrap();
    assert!(s.command(&v, false).is_none());
    let mut changed = v.clone();
    changed.has_control = false;
    assert!(s.command(&changed, true).is_none());
    changed = v.clone();
    changed.can_act = false;
    assert!(s.command(&changed, true).is_none());
    changed = v.clone();
    changed.visible[3].wall = true;
    assert!(s.command(&changed, true).is_none());
    assert_eq!(v.visible.len(), 4);
}
#[test]
fn selections_cannot_cross_actor_stream_branch_revision_or_readiness_changes() {
    let v = fixture();
    let s = MapSelection::select(&v, "opaque:1:0").unwrap();
    for n in 0..6 {
        let mut changed = v.clone();
        match n {
            0 => changed.shown.actor = ActorId(99),
            1 => changed.shown.branch = BranchId("other".into()),
            2 => changed.shown.input.stream.epoch += 1,
            3 => changed.shown.revision += 1,
            4 => changed.shown.input.readiness_revision += 1,
            _ => changed.shown.cursor.sequence += 1,
        };
        assert!(s.command(&changed, true).is_none(), "context mutation{n}");
    }
}
