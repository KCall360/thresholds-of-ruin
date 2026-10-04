use tor_protocol::*;

fn cell(x: i32, y: i32, z: i32, key: &str) -> CellView {
    CellView {
        asset: None,
        door: None,
        material: "stone".into(),
        key: key.into(),
        stairs_up: false,
        stairs_down: false,
        position: Position { x, y, z },
        wall: false,
        place_hint: false,
    }
}

/// A room of keyed cells seen from `(ox, oy)`, as observer-relative offsets.
fn room(ox: i32, oy: i32, revision: u64) -> StateView {
    let mut cells = Vec::new();
    for x in -6..=6 {
        for y in -6..=6 {
            let (wx, wy) = (x + ox, y + oy);
            if (0..30).contains(&wx) && (0..30).contains(&wy) {
                cells.push(cell(x, y, 0, &format!("{wx},{wy}")));
            }
        }
    }
    cells.sort_by_key(|c| c.position);
    StateView {
        wizard_game: false,
        revision,
        observation: Observation {
            combat: None,
            motion: None,
            places: Vec::new(),
            actor: ActorId(1),
            tick: revision,
            position: Position { x: 0, y: 0, z: 0 },
            visible_cells: cells,
            ground_items: Vec::new(),
            inventory: Vec::new(),
            visible_actors: Vec::new(),
            ready: true,
        },
    }
}

fn round_trip(base: &StateView, next: &StateView) -> StateDelta {
    let delta = StateDelta::between(base, next).expect("delta");
    let wire: StateDelta = serde_json::from_str(&serde_json::to_string(&delta).unwrap()).unwrap();
    assert_eq!(wire.clone().apply(base).unwrap(), *next);
    wire
}

#[test]
fn a_step_shifts_retained_cells_and_sends_only_the_edge() {
    let base = room(15, 15, 1);
    let next = room(16, 15, 2);
    let delta = round_trip(&base, &next);
    assert_eq!(delta.cells.shift, Position { x: -1, y: 0, z: 0 });
    assert_eq!(delta.cells.changed.len(), 13);
    assert_eq!(delta.cells.removed.len(), 13);
}

#[test]
fn changed_cells_are_resent_in_place() {
    let base = room(15, 15, 1);
    let mut next = room(15, 15, 2);
    next.observation.visible_cells[3].door = Some(DoorView {
        asset: None,
        id: 1,
        name: "wooden door".into(),
        description: String::new(),
        open: true,
        reachable: true,
        approaches: vec!["4,4".into()],
    });
    next.observation.visible_cells[7].wall = true;
    let delta = round_trip(&base, &next);
    assert_eq!(delta.cells.shift, Position { x: 0, y: 0, z: 0 });
    assert_eq!(delta.cells.changed.len(), 2);
    assert!(delta.cells.removed.is_empty());
}

#[test]
fn a_repeated_key_is_kept_apart_by_position() {
    // The same location seen twice, as through a link back into the room.
    let mut base = room(15, 15, 1);
    base.observation.visible_cells.push(cell(20, 0, 0, "15,15"));
    let mut next = room(15, 15, 2);
    next.observation.visible_cells.push(cell(20, 1, 0, "15,15"));
    let delta = round_trip(&base, &next);
    assert_eq!(delta.cells.removed, vec![Position { x: 20, y: 0, z: 0 }]);
    assert_eq!(delta.cells.changed, vec![cell(20, 1, 0, "15,15")]);
}

#[test]
fn unrelated_views_are_sent_in_full() {
    let base = room(15, 15, 1);
    let mut next = room(15, 15, 2);
    for cell in &mut next.observation.visible_cells {
        cell.key.push('!');
    }
    assert_eq!(StateDelta::between(&base, &next), None);
}

#[test]
fn unsorted_cells_are_sent_in_full() {
    let base = room(15, 15, 1);
    let mut next = room(15, 15, 2);
    next.observation.visible_cells.swap(0, 1);
    assert_eq!(StateDelta::between(&base, &next), None);
}

#[test]
fn a_delta_rejects_any_other_base() {
    let base = room(15, 15, 1);
    let delta = StateDelta::between(&base, &room(16, 15, 2)).unwrap();
    assert_eq!(
        delta.clone().apply(&room(15, 15, 7)),
        Err(DeltaError::WrongBase)
    );
    // Same revision, different contents: the removals no longer fit.
    let mut other = room(10, 10, 1);
    other.observation.visible_cells.truncate(10);
    assert_eq!(delta.apply(&other), Err(DeltaError::InvalidChange));
}

#[test]
fn malformed_changes_are_rejected() {
    let base = room(15, 15, 1);
    let mut delta = StateDelta::between(&base, &room(16, 15, 2)).unwrap();
    let removed = delta.cells.removed[0];
    delta
        .cells
        .changed
        .insert(0, cell(removed.x, removed.y, 0, "x"));
    delta.cells.changed.sort_by_key(|c| c.position);
    assert_eq!(delta.apply(&base), Err(DeltaError::InvalidChange));

    let mut delta = StateDelta::between(&base, &room(16, 15, 2)).unwrap();
    delta.cells.removed.push(Position { x: 99, y: 0, z: 0 });
    assert_eq!(delta.apply(&base), Err(DeltaError::InvalidChange));
}

fn axis_position(axis: usize, coordinate: i32, offset: i32) -> Position {
    match axis {
        0 => Position {
            x: coordinate,
            y: offset,
            z: 0,
        },
        1 => Position {
            x: offset,
            y: coordinate,
            z: 0,
        },
        2 => Position {
            x: offset,
            y: 0,
            z: coordinate,
        },
        _ => unreachable!(),
    }
}

fn edge_view(axis: usize, coordinate: i32, revision: u64) -> StateView {
    let mut state = room(15, 15, revision);
    state.observation.visible_cells = (0..8)
        .map(|offset| {
            let position = axis_position(axis, coordinate, offset);
            cell(
                position.x,
                position.y,
                position.z,
                &format!("edge-{offset}"),
            )
        })
        .collect();
    state
}

#[test]
fn delta_translation_rejects_coordinate_overflow_on_every_axis() {
    for axis in 0..3 {
        for (coordinate, step) in [(i32::MAX, 1), (i32::MIN, -1)] {
            let base = edge_view(axis, coordinate, 1);
            let mut delta = StateDelta::between(&base, &edge_view(axis, coordinate, 2)).unwrap();
            delta.cells.shift = axis_position(axis, step, 0);
            assert_eq!(
                delta.apply(&base),
                Err(DeltaError::InvalidChange),
                "axis {axis}, step {step}"
            );
        }
    }
}

#[test]
fn delta_generation_falls_back_when_translation_is_unrepresentable() {
    for axis in 0..3 {
        for (from, to) in [(i32::MAX, i32::MIN), (i32::MIN, i32::MAX)] {
            let base = edge_view(axis, from, 1);
            let next = edge_view(axis, to, 2);
            assert_eq!(StateDelta::between(&base, &next), None, "axis {axis}");
            let full: StateView =
                serde_json::from_value(serde_json::to_value(&next).unwrap()).unwrap();
            assert_eq!(full, next);
        }
    }
}

#[test]
fn delta_translation_preserves_representable_coordinate_limits() {
    for axis in 0..3 {
        for (from, to) in [(i32::MAX - 1, i32::MAX), (i32::MIN + 1, i32::MIN)] {
            round_trip(&edge_view(axis, from, 1), &edge_view(axis, to, 2));
        }
    }
}

#[test]
fn delta_generation_does_not_wrap_cells_that_would_be_removed() {
    let mut base = edge_view(0, 0, 1);
    base.observation
        .visible_cells
        .push(cell(i32::MAX, 0, 0, "removed"));
    let next = edge_view(0, 1, 2);
    assert_eq!(StateDelta::between(&base, &next), None);
}
