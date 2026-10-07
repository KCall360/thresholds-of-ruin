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
fn unrelated_views_still_have_an_exact_candidate() {
    let base = room(15, 15, 1);
    let mut next = room(15, 15, 2);
    for cell in &mut next.observation.visible_cells {
        cell.key.push('!');
    }
    let delta = StateDelta::between(&base, &next).unwrap();
    assert_eq!(delta.apply(&base).unwrap(), next);
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
fn complete_encoding_falls_back_when_translation_is_unrepresentable() {
    for axis in 0..3 {
        for (from, to) in [(i32::MAX, i32::MIN), (i32::MIN, i32::MAX)] {
            let base = edge_view(axis, from, 1);
            let next = edge_view(axis, to, 2);
            // Zero shift with explicit replacement remains representable;
            // an overflowing translation must never wrap either coordinate.
            let candidate = StateDelta::between(&base, &next).unwrap();
            assert_eq!(candidate.cells.shift, Position { x: 0, y: 0, z: 0 });
            assert_eq!(candidate.apply(&base).unwrap(), next);
            let message = complete_observation(next.clone(), 2);
            let selected = encode_response(
                &message,
                Some((exact_base(&base, 1), &base)),
                MAX_RESPONSE_BYTES,
            )
            .unwrap();
            assert_eq!(selected.observation, Some(ObservationEncoding::Full));
            assert_eq!(selected.text, serde_json::to_string(&message).unwrap());
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

fn complete_observation(state: StateView, sequence: u64) -> ServerMessage {
    ServerMessage::Update {
        update: Box::new(StreamUpdate {
            context: StreamContext {
                stream: StreamId("encoding-test".into()),
                epoch: 1,
            },
            actor: state.observation.actor,
            branch: BranchId("encoding-branch".into()),
            cursor: StreamCursor {
                sequence,
                tick: state.observation.tick,
            },
            body: UpdateBody::Observation {
                state: Box::new(state),
                event: None,
            },
        }),
    }
}

fn exact_base(state: &StateView, sequence: u64) -> ObservationBase {
    ObservationBase {
        cursor: StreamCursor {
            sequence,
            tick: state.observation.tick,
        },
        revision: state.revision,
    }
}

#[test]
fn complete_encoding_selects_exact_bytes_and_handles_both_frame_ceilings() {
    let mut base = room(15, 15, 1);
    base.observation.visible_cells[0].material = "é\"\\\n".repeat(128);
    let mut next = base.clone();
    next.revision = 2;
    next.observation.tick = 2;
    let message = complete_observation(next.clone(), 2);
    let previous = exact_base(&base, 1);
    let selected = encode_response(&message, Some((previous, &base)), MAX_RESPONSE_BYTES).unwrap();
    assert_eq!(selected.observation, Some(ObservationEncoding::Delta));
    let full = serde_json::to_string(&message).unwrap();
    assert!(selected.text.len() < full.len());
    let exact = encode_response(&message, Some((previous, &base)), selected.text.len()).unwrap();
    assert_eq!(
        exact.text, selected.text,
        "a delta fits even when full does not"
    );
    assert!(encode_response(&message, Some((previous, &base)), selected.text.len() - 1).is_err());
    let ServerMessage::Update { update } = decode_response(&exact.text).unwrap() else {
        panic!("update")
    };
    let UpdateBody::ObservationDelta {
        base: actual_base,
        state,
        ..
    } = update.body
    else {
        panic!("delta")
    };
    assert_eq!(actual_base, previous);
    assert_eq!(state.apply(&base).unwrap(), next);
    let direct = encode_response(&message, None, full.len()).unwrap();
    assert_eq!(direct.observation, Some(ObservationEncoding::Full));
    assert_eq!(direct.text, full);
    assert!(encode_response(&message, None, full.len() - 1).is_err());
}

#[test]
fn complete_encoding_prefers_full_when_every_cell_changes_or_base_is_wrong() {
    let base = room(15, 15, 1);
    let mut next = room(15, 15, 2);
    for cell in &mut next.observation.visible_cells {
        cell.key.push('!');
    }
    let message = complete_observation(next, 2);
    let full = serde_json::to_string(&message).unwrap();
    let selected =
        encode_response(&message, Some((exact_base(&base, 1), &base)), full.len()).unwrap();
    assert_eq!(selected.observation, Some(ObservationEncoding::Full));
    assert_eq!(selected.text, full);
    let next = room(15, 15, 2);
    let message = complete_observation(next, 2);
    for wrong in [
        ObservationBase {
            revision: 99,
            ..exact_base(&base, 1)
        },
        ObservationBase {
            cursor: StreamCursor {
                tick: 99,
                sequence: 1,
            },
            ..exact_base(&base, 1)
        },
    ] {
        let selected = encode_response(&message, Some((wrong, &base)), MAX_RESPONSE_BYTES).unwrap();
        assert_eq!(selected.observation, Some(ObservationEncoding::Full));
        assert_eq!(selected.text, serde_json::to_string(&message).unwrap());
    }
    let mut other = base.clone();
    other.observation.actor = ActorId(2);
    let selected = encode_response(
        &message,
        Some((exact_base(&other, 1), &other)),
        MAX_RESPONSE_BYTES,
    )
    .unwrap();
    assert_eq!(selected.observation, Some(ObservationEncoding::Full));
}

#[test]
fn equal_complete_encoded_sizes_prefer_full() {
    let mut found = false;
    for key_length in 1..=256 {
        let mut base = room(15, 15, 9_000_000_000_000_000_000);
        base.observation.visible_cells.truncate(1);
        base.observation.visible_cells[0].key = "k".repeat(key_length);
        let mut next = base.clone();
        next.revision += 1;
        next.observation.tick += 1;
        let previous = exact_base(&base, base.revision);
        let full = complete_observation(next.clone(), next.revision);
        let mut candidate = full.clone();
        let ServerMessage::Update { update } = &mut candidate else {
            unreachable!()
        };
        update.body = UpdateBody::ObservationDelta {
            base: previous,
            state: Box::new(StateDelta::between(&base, &next).unwrap()),
            event: None,
        };
        if serde_json::to_vec(&candidate).unwrap().len() == serde_json::to_vec(&full).unwrap().len()
        {
            let selected =
                encode_response(&full, Some((previous, &base)), MAX_RESPONSE_BYTES).unwrap();
            assert_eq!(selected.observation, Some(ObservationEncoding::Full));
            assert_eq!(selected.text, serde_json::to_string(&full).unwrap());
            found = true;
            break;
        }
    }
    assert!(
        found,
        "the fixture must cover an actual equal-size boundary"
    );
}
