use std::sync::Arc;
use tor_protocol::*;

// Synthetic disclosed identities for wire-only tests; not server target derivation.
fn synthetic_digest(index: u64) -> [u8; 32] {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&index.to_le_bytes());
    digest
}

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
            interactions: None,
            combat: None,
            motion: None,
            places: Vec::new(),
            actor: ActorId(1),
            self_target: ActorTarget::from_digest(synthetic_digest(1)),
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
fn interaction_snapshots_round_trip_delta_and_validate_anatomy_bounds() {
    let base = room(5, 5, 1);
    let mut next = base.clone();
    next.revision = 2;
    next.observation.interactions = Some(InteractionView {
        completed: vec![Action::Drink {
            item: ItemTarget::from_digest(synthetic_digest(3)),
        }],
        slots: vec![EquipmentSlot::Ring, EquipmentSlot::Ring],
        preparation: Some(PreparationView {
            action: Action::Attack {
                target: ActorTarget::from_digest(synthetic_digest(2)),
            },
            remaining: u64::MAX,
            active: false,
        }),
        inventory: vec![],
    });
    assert!(next.validate().is_ok());
    round_trip(&base, &next);
    let mut invalid = next.clone();
    invalid
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .completed
        .push(Action::Wait);
    assert!(
        invalid.validate().is_err(),
        "completion must describe an item action"
    );
    let mut cleared = next.clone();
    cleared.revision = 3;
    cleared.observation.interactions = None;
    round_trip(&next, &cleared);
    next.observation.interactions.as_mut().unwrap().slots = vec![EquipmentSlot::Ring; 65];
    assert!(
        next.validate().is_err(),
        "unbounded anatomy must be rejected"
    );
}

#[test]
fn a_delta_cannot_cross_an_observers_entity_target_scope() {
    let base = room(15, 15, 1);
    let mut next = room(15, 15, 2);
    next.observation.self_target = ActorTarget::from_digest([42; 32]);
    assert!(StateDelta::between(&base, &next).is_none());
    next.observation.self_target = base.observation.self_target;
    let mut delta = round_trip(&base, &next);
    delta.self_target = ActorTarget::from_digest([42; 32]);
    assert!(delta.apply(&base).is_err());
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
        id: DoorTarget::from_digest(synthetic_digest(1)),
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
                state: state.into(),
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

fn collection_view(count: usize) -> StateView {
    let mut state = room(15, 15, 1);
    for index in 0..count {
        let item = ItemView {
            class: Default::default(),
            quantity: 1,
            appearance: "disclosed appearance".repeat(4),
            identified: true,
            description: "disclosed description".repeat(4),
            id: ItemTarget::from_digest(synthetic_digest(index as u64 + 1)),
            name: format!("carried item {index}"),
            asset: None,
        };
        state.observation.inventory.push(item.clone());
        state.observation.ground_items.push(GroundItemView {
            reachable: false,
            item: ItemView {
                id: ItemTarget::from_digest(synthetic_digest(index as u64 + 10_000)),
                ..item
            },
            position: Position {
                x: index as i32,
                y: 1,
                z: 0,
            },
        });
        state.observation.visible_actors.push(ActorView {
            name: format!("actor {index}"),
            description: "disclosed actor appearance".repeat(4),
            id: ActorTarget::from_digest(synthetic_digest(index as u64 + 10)),
            position: Position {
                x: index as i32,
                y: 2,
                z: 0,
            },
            asset: None,
        });
        state.observation.places.push(PlaceView {
            key: format!("opaque-place-{index}"),
            name: "remembered place name".repeat(4),
            origin: PlaceNameOrigin::Invented,
        });
    }
    state.validate().unwrap();
    state
}

fn encoded_collection_update(base: &StateView, next: &StateView) -> usize {
    next.validate().unwrap();
    let message = complete_observation(next.clone(), 2);
    let encoded = encode_response(
        &message,
        Some((exact_base(base, 1), base)),
        MAX_RESPONSE_BYTES,
    )
    .unwrap();
    let ServerMessage::Update { update } = decode_response(&encoded.text).unwrap() else {
        panic!("update")
    };
    let reconstructed = match update.body {
        UpdateBody::Observation { state, .. } => Arc::unwrap_or_clone(state),
        UpdateBody::ObservationDelta { state, .. } => state.apply(base).unwrap(),
        _ => panic!("observation"),
    };
    reconstructed.validate().unwrap();
    assert_eq!(reconstructed, *next);
    encoded.text.len()
}

#[test]
fn unchanged_collection_update_bytes_do_not_grow_with_retained_contents() {
    let mut lengths = Vec::new();
    for count in [16, 256, 4096] {
        let base = collection_view(count);
        let mut next = base.clone();
        next.revision += 1;
        next.observation.tick += 1;
        next.observation.visible_cells[3].wall = true;
        lengths.push(encoded_collection_update(&base, &next));
    }
    assert!(
        lengths.iter().all(|bytes| *bytes < 2048),
        "complete update sizes: {lengths:?}"
    );
    assert_eq!(
        lengths[0], lengths[2],
        "retained collections must have no wire payload"
    );
}

#[test]
fn separated_collection_edits_send_changed_values_without_retained_contents() {
    let base = collection_view(4096);
    let mut next = base.clone();
    next.revision += 1;
    next.observation.tick += 1;
    for index in [3, 2000, 4090] {
        next.observation.inventory[index].quantity += 1;
        next.observation.ground_items[index].reachable = true;
        next.observation.visible_actors[index].description.push('!');
        next.observation.places[index].name.push('!');
    }
    let bytes = encoded_collection_update(&base, &next);
    assert!(bytes < 8192, "sparse collection update used {bytes} bytes");
}

#[test]
fn collection_updates_keep_repeated_portal_entities_at_distinct_offsets() {
    let mut base = collection_view(256);
    let mut ground = base.observation.ground_items[0].clone();
    ground.position.y = -1;
    base.observation.ground_items.insert(100, ground);
    let mut actor = base.observation.visible_actors[0].clone();
    actor.position.y = -2;
    base.observation.visible_actors.insert(100, actor);
    base.validate().unwrap();
    let mut next = base.clone();
    next.revision += 1;
    next.observation.tick += 1;
    next.observation.ground_items[100].reachable = true;
    next.observation.visible_actors.remove(100);
    let bytes = encoded_collection_update(&base, &next);
    assert!(
        bytes < 2048,
        "one projected occurrence update used {bytes} bytes"
    );
}

#[test]
fn ordered_collection_edits_reconstruct_reorders_insertions_and_empty_transitions() {
    let base = collection_view(16);
    for case in 0..6 {
        let mut next = base.clone();
        next.revision += 1;
        next.observation.tick += 1;
        match case {
            0 => {
                next.observation.inventory.reverse();
                next.observation.ground_items.reverse();
                next.observation.visible_actors.reverse();
                next.observation.places.reverse();
            }
            1 => {
                next.observation.inventory.rotate_left(7);
                next.observation.ground_items.rotate_right(3);
                next.observation.visible_actors.swap(0, 15);
                next.observation.places.swap(5, 10);
            }
            2 => {
                next.observation.inventory.clear();
                next.observation.ground_items.clear();
                next.observation.visible_actors.clear();
                next.observation.places.clear();
            }
            3 => {
                next.observation.inventory.remove(2);
                next.observation.ground_items.remove(3);
                next.observation.visible_actors.remove(4);
                next.observation.places.remove(5);
            }
            4 => {
                let mut extra = next.observation.inventory[0].clone();
                extra.id = ItemTarget::from_digest(synthetic_digest(100_000));
                next.observation.inventory.insert(8, extra);
                let mut extra = next.observation.ground_items[0].clone();
                extra.position.z = 3;
                next.observation.ground_items.insert(8, extra);
                let mut extra = next.observation.visible_actors[0].clone();
                extra.position.z = 3;
                next.observation.visible_actors.insert(8, extra);
                let mut extra = next.observation.places[0].clone();
                extra.key = "new-opaque-place".into();
                next.observation.places.insert(8, extra);
            }
            _ => {}
        }
        next.validate().unwrap();
        round_trip(&base, &next);
        // Reversal is intentionally valid: a full view need not be sorted.
        round_trip(&next, &base);
    }
}

#[test]
fn ordered_collection_edits_reject_invalid_original_base_ranges() {
    let base = collection_view(16);
    let mut next = base.clone();
    next.revision += 1;
    next.observation.tick += 1;
    let candidate = StateDelta::between(&base, &next).unwrap();
    let edit = |start, remove| CollectionEdit::<ItemView> {
        start,
        remove,
        insert: Vec::new(),
    };
    for invalid in [
        vec![edit(17, 1)],
        vec![edit(16, 1)],
        vec![edit(1, u32::MAX)],
        vec![edit(u32::MAX, u32::MAX)],
        vec![edit(0, 0)],
        vec![edit(2, 2), edit(3, 1)],
        vec![edit(5, 1), edit(2, 1)],
        vec![edit(2, 1), edit(2, 1)],
    ] {
        let mut delta = candidate.clone();
        delta.inventory = invalid;
        assert_eq!(delta.apply(&base), Err(DeltaError::InvalidChange));
    }
    assert_eq!(base, collection_view(16));
    let encoded = serde_json::to_value(edit(0, 1)).unwrap();
    let mut unknown = encoded;
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<CollectionEdit<ItemView>>(unknown).is_err());
    for malformed in [
        serde_json::json!(-1),
        serde_json::json!(4294967296u64),
        serde_json::json!("1"),
    ] {
        let mut value = serde_json::to_value(edit(0, 1)).unwrap();
        value["start"] = malformed;
        assert!(serde_json::from_value::<CollectionEdit<ItemView>>(value).is_err());
    }
}

#[test]
fn complete_encoding_rejects_a_fitting_delta_when_retained_state_would_exceed_the_ceiling() {
    let mut base = collection_view(1);
    base.observation.inventory[0].description = "x".repeat(MAX_RESPONSE_BYTES / 2);
    base.validate().unwrap();
    let mut next = base.clone();
    next.revision += 1;
    next.observation.tick += 1;
    next.observation.ground_items[0].item.description = "y".repeat(MAX_RESPONSE_BYTES / 2);
    let message = complete_observation(next, 2);
    assert!(matches!(
        encode_response(
            &message,
            Some((exact_base(&base, 1), &base)),
            MAX_RESPONSE_BYTES
        ),
        Err(EncodeError::RetainedStateTooLarge {
            limit: MAX_STATE_BYTES
        })
    ));
}
