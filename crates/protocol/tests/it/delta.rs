use std::sync::Arc;
use tor_protocol::*;

#[test]
fn illumination_is_required_and_changes_without_changing_visibility() {
    let base = room(5, 5, 1);
    let mut next = base.clone();
    next.revision += 1;
    next.observation.visible_cells[0].lit = false;
    let delta = round_trip(&base, &next);
    assert_eq!(
        delta.cells.changed,
        vec![next.observation.visible_cells[0].clone()]
    );
    assert!(delta.cells.removed.is_empty());
    let mut wire = serde_json::to_value(&next.observation.visible_cells[0]).unwrap();
    assert_eq!(wire["lit"], false);
    wire.as_object_mut().unwrap().remove("lit");
    assert!(serde_json::from_value::<CellView>(wire.clone()).is_err());
    wire["lit"] = serde_json::json!("false");
    assert!(serde_json::from_value::<CellView>(wire).is_err());
}

fn equipment_slot(id: &str, requires_attack: bool) -> EquipmentSlot {
    EquipmentSlot {
        id: id.into(),
        name: "Fixture socket".into(),
        equipped_phrase: "fastened to a wing".into(),
        requires_attack,
    }
}

#[test]
fn appearance_and_corpse_disclosures_survive_full_and_delta_round_trips() {
    let base = room(5, 5, 1);
    let mut next = base.clone();
    next.revision += 1;
    let appearance = CreatureAppearance {
        seed: AppearanceSeed::from_digest([19; 32]),
        asset: Some("creature.equine".into()),
    };
    next.observation.self_appearance = Some(appearance.clone());
    next.observation.inventory.push(ItemView {
        corpse: Some(appearance),
        class: ItemClass::Corpse,
        quantity: 1,
        id: ItemTarget::from_digest([20; 32]),
        name: "unrelated label".into(),
        appearance: "body".into(),
        identified: true,
        description: String::new(),
        asset: Some("item.corpse".into()),
        known_currency: None,
        known_supply: None,
    });
    round_trip(&base, &next);
    let encoded = serde_json::to_value(&next).unwrap();
    assert_eq!(serde_json::from_value::<StateView>(encoded).unwrap(), next);
    let good = next.observation.inventory.last().unwrap().clone();
    for (class, identified) in [(ItemClass::Weapon, true), (ItemClass::Corpse, false)] {
        let mut bad = good.clone();
        bad.class = class;
        bad.identified = identified;
        assert!(serde_json::from_value::<ItemView>(serde_json::to_value(&bad).unwrap()).is_err());
        next.observation.inventory[0] = bad;
        assert_eq!(next.validate(), Err(InvalidState::InvalidItem));
    }
}

fn equipped_state(requires_attack: bool) -> StateView {
    let mut state = room(5, 5, 1);
    let slot = equipment_slot("fixture_wing", requires_attack);
    let item = ItemTarget::from_digest(synthetic_digest(3));
    state.observation.inventory = vec![ItemView {
        corpse: None,
        id: item,
        name: "Wing fitting".into(),
        appearance: "a brass fitting".into(),
        description: String::new(),
        class: ItemClass::Misc,
        quantity: 1,
        identified: true,
        asset: None,
        known_currency: None,
        known_supply: None,
    }];
    state.observation.interactions = Some(InteractionView {
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![slot.clone()],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item,
            slot: Some(slot),
            equipped_slot: Some(0),
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: None,
            known_equipment: Some(EquipmentView {
                attack: requires_attack.then(super::creature_inspection::attack),
                modifiers: vec![],
            }),
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            charges: None,
        }],
    });
    state
}

#[test]
fn known_currency_survives_full_and_delta_updates_and_rejects_hidden_facts() {
    let mut base = room(5, 5, 1);
    base.observation.inventory.push(ItemView {
        corpse: None,
        id: ItemTarget::from_digest([83; 32]),
        class: ItemClass::Coin,
        quantity: 9_007_199_254_740_993,
        appearance: "unmarked discs".into(),
        identified: false,
        description: String::new(),
        name: "unmarked discs".into(),
        asset: None,
        known_supply: None,
        known_currency: None,
    });
    let mut next = base.clone();
    next.revision += 1;
    next.observation.inventory[0].identified = true;
    next.observation.inventory[0].known_currency = Some(CurrencyKind::Gold);
    next.validate().unwrap();
    round_trip(&base, &next);
    assert_eq!(
        serde_json::from_value::<StateView>(serde_json::to_value(&next).unwrap()).unwrap(),
        next
    );
    let mut changed = next.clone();
    changed.revision += 1;
    changed.observation.inventory[0].quantity -= 1;
    round_trip(&next, &changed);
    for (class, identified) in [(ItemClass::Coin, false), (ItemClass::Gem, true)] {
        let mut invalid = next.clone();
        invalid.observation.inventory[0].class = class;
        invalid.observation.inventory[0].identified = identified;
        assert_eq!(invalid.validate(), Err(InvalidState::InvalidItem));
        let delta = StateDelta::between(&base, &invalid).unwrap();
        assert!(
            serde_json::from_value::<StateDelta>(serde_json::to_value(delta).unwrap()).is_err()
        );
    }
}

// Synthetic disclosed identities for wire-only tests; not server target derivation.
fn synthetic_digest(index: u64) -> [u8; 32] {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&index.to_le_bytes());
    digest
}

fn cell(x: i32, y: i32, z: i32, key: &str) -> CellView {
    CellView {
        lit: true,
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
            self_appearance: None,
            interactions: None,
            combat: None,
            motion: None,
            places: Vec::new(),
            actor: ActorId(1),
            self_target: ActorTarget::from_digest(synthetic_digest(1)),
            linked_holds: vec![],
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
fn prepared_ability_snapshots_validate_and_round_trip_deltas() {
    let base = room(5, 5, 1);
    for ability in ["basic_melee", "fixture_ability", "fear"] {
        let mut next = base.clone();
        next.revision = 2;
        next.observation.interactions = Some(InteractionView {
            curse_blocked: vec![],
            identification: None,
            completed: vec![],
            slots: vec![],
            inventory: vec![],
            preparation: Some(PreparationView {
                action: Action::UseAbility {
                    id: ability.into(),
                    invocation: tor_protocol::AbilityInvocation {
                        aim: tor_protocol::AbilityAim::ActorTarget {
                            target: ActorTarget::from_digest(synthetic_digest(2)),
                        },
                        weapon: tor_protocol::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
                remaining: 100,
                active: true,
            }),
        });
        assert_eq!(next.validate(), Ok(()));
        round_trip(&base, &next);
        next.observation
            .interactions
            .as_mut()
            .unwrap()
            .preparation
            .as_mut()
            .unwrap()
            .action = Action::Wait;
        assert_eq!(next.validate(), Err(InvalidState::InvalidInteraction));
    }
}

#[test]
fn interaction_snapshots_round_trip_delta_and_validate_anatomy_bounds() {
    let base = room(5, 5, 1);
    let mut next = base.clone();
    next.revision = 2;
    next.observation.interactions = Some(InteractionView {
        curse_blocked: vec![],
        identification: None,
        completed: vec![Action::Drink {
            item: ItemTarget::from_digest(synthetic_digest(3)),
        }],
        slots: vec![equipment_slot("ring", false), equipment_slot("ring", false)],
        preparation: Some(PreparationView {
            action: Action::UseAbility {
                id: "basic_melee".into(),
                invocation: tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::ActorTarget {
                        target: ActorTarget::from_digest(synthetic_digest(2)),
                    },
                    weapon: tor_protocol::WeaponSelection::Default,
                },
                supplies: Default::default(),
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
    next.observation.interactions.as_mut().unwrap().slots = vec![equipment_slot("ring", false); 65];
    assert!(
        next.validate().is_err(),
        "unbounded anatomy must be rejected"
    );
}

#[test]
fn custom_equipment_descriptors_round_trip_with_and_without_attacks() {
    for requires_attack in [false, true] {
        let state = equipped_state(requires_attack);
        assert_eq!(state.validate(), Ok(()));
        let wire = serde_json::to_value(&state).unwrap();
        let slots = &wire["observation"]["interactions"]["slots"];
        assert_eq!(slots[0]["id"], "fixture_wing");
        assert_eq!(slots[0]["name"], "Fixture socket");
        assert_eq!(slots[0]["equipped_phrase"], "fastened to a wing");
        assert_eq!(slots[0]["requires_attack"], requires_attack);
        assert_eq!(serde_json::from_value::<StateView>(wire).unwrap(), state);
        round_trip(&room(5, 5, 0), &state);
    }
}

#[test]
fn invalid_equipment_descriptors_are_rejected_in_anatomy_and_inventory() {
    let valid = equipped_state(false);
    for anatomy in [false, true] {
        for field in ["id", "name", "equipped_phrase"] {
            for invalid_text in ["", " ", "bad\nlabel"] {
                let mut invalid = valid.clone();
                let interactions = invalid.observation.interactions.as_mut().unwrap();
                // Validate each descriptor even when no equipped socket links them.
                interactions.inventory[0].equipped_slot = None;
                let slot = if anatomy {
                    &mut interactions.slots[0]
                } else {
                    interactions.inventory[0].slot.as_mut().unwrap()
                };
                match field {
                    "id" => slot.id = invalid_text.into(),
                    "name" => slot.name = invalid_text.into(),
                    _ => slot.equipped_phrase = invalid_text.into(),
                }
                assert_eq!(
                    invalid.validate(),
                    Err(InvalidState::InvalidInteraction),
                    "anatomy={anatomy}, field={field}, text={invalid_text:?}"
                );
            }
        }
        for (field, invalid_text) in [
            ("id", "x".repeat(61)),
            ("name", "x".repeat(257)),
            ("equipped_phrase", "x".repeat(257)),
        ] {
            let mut invalid = valid.clone();
            let interactions = invalid.observation.interactions.as_mut().unwrap();
            interactions.inventory[0].equipped_slot = None;
            let slot = if anatomy {
                &mut interactions.slots[0]
            } else {
                interactions.inventory[0].slot.as_mut().unwrap()
            };
            match field {
                "id" => slot.id = invalid_text,
                "name" => slot.name = invalid_text,
                _ => slot.equipped_phrase = invalid_text,
            }
            assert_eq!(
                invalid.validate(),
                Err(InvalidState::InvalidInteraction),
                "anatomy={anatomy}, field={field}"
            );
        }
    }
}

#[test]
fn equipped_items_require_the_complete_socket_descriptor() {
    let valid = equipped_state(false);
    for field in ["id", "name", "equipped_phrase", "requires_attack"] {
        let mut invalid = valid.clone();
        let slot = &mut invalid.observation.interactions.as_mut().unwrap().slots[0];
        match field {
            "id" => slot.id = "another_socket".into(),
            "name" => slot.name = "Another name".into(),
            "equipped_phrase" => slot.equipped_phrase = "balanced on a horn".into(),
            _ => slot.requires_attack = true,
        }
        assert_eq!(
            invalid.validate(),
            Err(InvalidState::InvalidInteraction),
            "{field}"
        );
    }
}

#[test]
fn known_equipment_attack_presence_must_match_the_disclosed_requirement() {
    for requires_attack in [false, true] {
        let mut invalid = equipped_state(requires_attack);
        invalid.observation.interactions.as_mut().unwrap().inventory[0]
            .known_equipment
            .as_mut()
            .unwrap()
            .attack = (!requires_attack).then(super::creature_inspection::attack);
        assert_eq!(invalid.validate(), Err(InvalidState::InvalidInteraction));
    }
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
            corpse: None,
            class: Default::default(),
            quantity: 1,
            appearance: "disclosed appearance".repeat(4),
            identified: true,
            description: "disclosed description".repeat(4),
            id: ItemTarget::from_digest(synthetic_digest(index as u64 + 1)),
            name: format!("carried item {index}"),
            asset: None,
            known_currency: None,
            known_supply: None,
        };
        state.observation.inventory.push(item.clone());
        state.observation.ground_items.push(GroundItemView {
            reachable: false,
            supply_reachable: false,
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
            appearance_seed: None,
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

#[test]
fn zero_hd_death_keeps_strict_combat_validation_and_delta_round_trip() {
    let base = room(5, 5, 1);
    let mut next = base.clone();
    next.revision = 2;
    next.observation.ready = false;
    let mut stats = super::creature::stats();
    stats.hit_dice = HitDiceSummary {
        total: 0,
        sources: vec![],
    };
    next.observation.combat = Some(CombatView {
        own_stats: Some(stats),
        public_plan: None,
        hp: 0,
        max_hp: 0,
        dead: true,
        preparation_remaining: None,
        sequence: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![],
        events: vec![],
        objective: None,
        exit: None,
        victory: false,
        terminal: false,
    });
    assert!(next.validate().is_ok());
    round_trip(&base, &next);
    for change in 0..5 {
        let mut invalid = next.clone();
        let combat = invalid.observation.combat.as_mut().unwrap();
        match change {
            0 => combat.own_stats = None,
            1 => {
                combat.own_stats.as_mut().unwrap().hit_dice = HitDiceSummary {
                    total: 1,
                    sources: vec![HitDiceSourceCount {
                        source_kind: HitDieSourceKind::Racial,
                        source: super::creature::content("racial"),
                        count: 1,
                    }],
                }
            }
            2 => combat.dead = false,
            3 => {
                combat.dead = false;
                combat.hp = 1;
                combat.max_hp = 1;
            }
            4 => combat.max_hp = 1,
            _ => unreachable!(),
        }
        assert_eq!(invalid.validate(), Err(InvalidState::InvalidCombat));
    }
}

#[test]
fn full_observations_reject_forged_summary_counts_before_replacing_state() {
    let mut valid = room(5, 5, 1);
    valid.observation.combat = Some(CombatView {
        own_stats: Some(super::creature::stats()),
        public_plan: None,
        hp: 10,
        max_hp: 10,
        dead: false,
        preparation_remaining: None,
        sequence: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![],
        events: vec![],
        objective: None,
        exit: None,
        victory: false,
        terminal: false,
    });
    valid.validate().unwrap();
    for case in 0..4 {
        let mut invalid = valid.clone();
        let summary = &mut invalid
            .observation
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .hit_dice;
        match case {
            0 => summary.total = 2,
            1 => summary.sources[0].count = 0,
            2 => {
                let mut duplicate = summary.sources[0].clone();
                duplicate.source.name = "Another label for the same identity".into();
                summary.sources.push(duplicate);
                summary.total = 2;
            }
            3 => {
                summary.total = u64::MAX;
                summary.sources[0].count = u64::MAX;
                let mut extra = summary.sources[0].clone();
                extra.source_kind = HitDieSourceKind::Class;
                extra.source = super::creature::content("overflow");
                extra.count = 1;
                summary.sources.insert(0, extra);
            }
            _ => unreachable!(),
        }
        assert_eq!(invalid.validate(), Err(InvalidState::InvalidCombat));
    }
}

// Fixed seeds and bounded traces keep failures reproducible without adding a
// random dependency. The expected state is edited directly, independently of
// the delta's original-base edit algorithm.
struct TraceRng(u64);

impl TraceRng {
    fn index(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

fn transform_values<T>(values: &mut Vec<T>, fresh: T, operation: usize, rng: &mut TraceRng) {
    match operation {
        0 => {
            let index = rng.index(values.len() + 1);
            values.insert(index, fresh);
        }
        1 if !values.is_empty() => {
            let index = rng.index(values.len());
            values.remove(index);
        }
        2 if !values.is_empty() => {
            let index = rng.index(values.len());
            values.rotate_left(index);
            values.reverse();
        }
        3 if !values.is_empty() => {
            let index = rng.index(values.len());
            values[index] = fresh;
        }
        4 => values.clear(),
        _ => {}
    }
    assert!(values.len() <= 32, "generated collections stay bounded");
}

fn transform_state(state: &mut StateView, serial: u64, operation: usize, rng: &mut TraceRng) {
    let template = collection_view(1);
    let mut item = template.observation.inventory[0].clone();
    item.id = ItemTarget::from_digest(synthetic_digest(100_000 + serial));
    let mut ground = template.observation.ground_items[0].clone();
    // Repeated portal identities remain valid at different projected offsets.
    ground.position.z = serial as i32 * 1000;
    let mut actor = template.observation.visible_actors[0].clone();
    actor.position.z = serial as i32 * 1000;
    let mut place = template.observation.places[0].clone();
    place.key = format!("generated-place-{serial}");
    let observation = &mut state.observation;
    transform_values(&mut observation.inventory, item, operation, rng);
    transform_values(&mut observation.ground_items, ground, operation, rng);
    transform_values(&mut observation.visible_actors, actor, operation, rng);
    transform_values(&mut observation.places, place, operation, rng);
    transform_values(
        &mut observation.visible_cells,
        cell(serial as i32 * 1000, 0, 0, "repeated-portal-cell"),
        operation,
        rng,
    );
    // Compose structural changes with content edits and cell translation.
    if let Some(item) = observation.inventory.first_mut() {
        item.quantity += 1;
    }
    if let Some(item) = observation.ground_items.last_mut() {
        item.reachable = !item.reachable;
    }
    if let Some(actor) = observation.visible_actors.first_mut() {
        actor.description.push('é');
    }
    if let Some(place) = observation.places.last_mut() {
        place.name.push('界');
    }
    let shift = rng.index(3) as i32 - 1;
    for cell in &mut observation.visible_cells {
        cell.position.y += shift;
    }
    if let Some(cell) = observation.visible_cells.first_mut() {
        cell.wall = !cell.wall;
    }
    observation.visible_cells.sort_by_key(|cell| cell.position);
    observation.ready = !observation.ready;
}

fn assert_generated_delta(base: &StateView, next: &StateView, trace: &str) -> StateDelta {
    assert_eq!(base.validate(), Ok(()), "base: {trace}");
    assert_eq!(next.validate(), Ok(()), "next: {trace}");
    let before = serde_json::to_vec(base).unwrap();
    let expected = serde_json::to_vec(next).unwrap();
    let delta = StateDelta::between(base, next).unwrap_or_else(|| panic!("no delta: {trace}"));
    let wire = serde_json::to_vec(&delta).unwrap();
    assert_eq!(
        wire,
        serde_json::to_vec(&StateDelta::between(base, next).unwrap()).unwrap(),
        "nondeterministic delta: {trace}"
    );
    let decoded: StateDelta = serde_json::from_slice(&wire).unwrap();
    assert_eq!(decoded.apply(base).unwrap(), *next, "{trace}");
    assert_eq!(
        serde_json::to_vec(base).unwrap(),
        before,
        "base changed: {trace}"
    );
    assert_eq!(
        serde_json::to_vec(next).unwrap(),
        expected,
        "next changed: {trace}"
    );
    delta
}

#[test]
fn seeded_composed_collection_and_cell_edits_match_materialized_states() {
    for seed in [1, 7, 19, 42, 73, 101, 257, 1021, 4093, 65537, 99991, 0x5eed] {
        let mut rng = TraceRng(seed);
        let mut initial = collection_view(8);
        initial.observation.visible_cells.truncate(8);
        let mut ground = initial.observation.ground_items[0].clone();
        ground.position.z = -20;
        initial.observation.ground_items.push(ground);
        let mut actor = initial.observation.visible_actors[0].clone();
        actor.position.z = -20;
        initial.observation.visible_actors.push(actor);
        let mut image = initial.observation.visible_cells[0].clone();
        image.position = Position {
            x: 20,
            y: 20,
            z: -20,
        };
        initial.observation.visible_cells.push(image);
        initial
            .observation
            .visible_cells
            .sort_by_key(|cell| cell.position);
        let mut base = initial.clone();
        let mut operations = Vec::new();
        for step in 0..24 {
            let mut next = base.clone();
            next.revision += 1;
            next.observation.tick += 1;
            // Two to four transformations precede each observed boundary.
            // Every trace includes empty transitions and later insertions.
            for part in 0..2 + rng.index(3) {
                let operation = if step % 11 == 10 { 4 } else { rng.index(4) };
                operations.push(operation);
                transform_state(&mut next, (step * 4 + part + 1) as u64, operation, &mut rng);
            }
            let trace = format!("seed={seed}, step={step}, operations={operations:?}");
            let delta = assert_generated_delta(&base, &next, &trace);
            assert_generated_delta(&next, &base, &trace);
            assert_generated_delta(&initial, &next, &trace);
            // Corrupt a different collection at each boundary. Even a late
            // failure, after valid cell/earlier collection edits, is atomic.
            let mut invalid = delta;
            let bad = |length| CollectionEdit {
                start: length as u32 + 1,
                remove: 1,
                insert: Vec::new(),
            };
            match step % 4 {
                0 => invalid.inventory = vec![bad(base.observation.inventory.len())],
                1 => {
                    invalid.ground_items = vec![CollectionEdit {
                        start: base.observation.ground_items.len() as u32 + 1,
                        remove: 1,
                        insert: vec![],
                    }]
                }
                2 => {
                    invalid.visible_actors = vec![CollectionEdit {
                        start: base.observation.visible_actors.len() as u32 + 1,
                        remove: 1,
                        insert: vec![],
                    }]
                }
                _ => {
                    invalid.places = vec![CollectionEdit {
                        start: base.observation.places.len() as u32 + 1,
                        remove: 1,
                        insert: vec![],
                    }]
                }
            }
            let before = base.clone();
            assert_eq!(
                invalid.apply(&base),
                Err(DeltaError::InvalidChange),
                "{trace}"
            );
            assert_eq!(base, before, "malformed delta changed input: {trace}");
            base = next;
        }
        for operation in 0..5 {
            assert!(
                operations.contains(&operation),
                "seed={seed}, missing operation={operation}"
            );
        }
    }
}

#[test]
fn seeded_collection_edits_compose_with_signed_coordinate_edges_and_full_fallback() {
    for seed in [1, 7, 19, 42, 73, 101] {
        let mut rng = TraceRng(seed);
        for axis in 0..3 {
            let coordinate = i32::MIN + rng.index(8) as i32;
            let mut base = collection_view(8);
            base.observation.visible_cells =
                edge_view(axis, coordinate, 1).observation.visible_cells;
            for coordinate in [coordinate + 1, i32::MAX - rng.index(8) as i32] {
                let mut next = base.clone();
                next.observation.visible_cells.clear();
                transform_state(&mut next, 100, 0, &mut rng);
                next.observation.visible_cells =
                    edge_view(axis, coordinate, 2).observation.visible_cells;
                next.revision += 1;
                next.observation.tick += 1;
                let trace = format!("seed={seed}, axis={axis}, coordinate={coordinate}");
                assert_eq!(next.validate(), Ok(()), "{trace}");
                if let Some(delta) = StateDelta::between(&base, &next) {
                    assert_eq!(delta.apply(&base).unwrap(), next, "{trace}");
                }
                // The public complete encoder must reconstruct exactly whether
                // it selects a delta or falls back for an unrepresentable shift.
                encoded_collection_update(&base, &next);
            }
        }
    }
}

#[test]
fn health_recovery_progress_validates_against_full_and_dead_combat_and_deltas() {
    let base = room(5, 5, 1);
    for (hp, dead) in [(10, false), (8, false), (0, true)] {
        let mut next = base.clone();
        next.revision = 2;
        next.observation.ready = !dead;
        next.observation.combat = Some(CombatView {
            own_stats: Some(super::creature::stats()),
            public_plan: None,
            hp,
            max_hp: 10,
            dead,
            preparation_remaining: None,
            sequence: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![],
            events: vec![],
            objective: None,
            exit: None,
            victory: false,
            terminal: false,
        });
        assert!(next.validate().is_ok());
        round_trip(&base, &next);
        next.observation
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .health_recovery
            .elapsed_ticks = 1;
        if hp == 8 {
            assert!(next.validate().is_ok());
            round_trip(&base, &next);
        } else {
            assert_eq!(next.validate(), Err(InvalidState::InvalidCombat));
            // Delta application reconstructs; the stream client validates the
            // complete observation before publishing it.
            assert_eq!(
                StateDelta::between(&base, &next)
                    .unwrap()
                    .apply(&base)
                    .unwrap()
                    .validate(),
                Err(InvalidState::InvalidCombat)
            );
        }
    }
}

#[test]
fn phase5_preparation_supply_shape_is_validated_for_direct_values() {
    let mut state = room(5, 5, 1);
    state.observation.interactions = Some(InteractionView {
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![],
        inventory: vec![],
        preparation: Some(PreparationView {
            action: Action::UseAbility {
                id: "mend".into(),
                invocation: tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::ActorTarget {
                        target: state.observation.self_target,
                    },
                    weapon: tor_protocol::WeaponSelection::Default,
                },
                supplies: SupplySelection::Carried,
            },
            remaining: 5,
            active: true,
        }),
    });
    assert_eq!(state.validate(), Ok(()));
    let row = SelectedSupply {
        cost_ordinal: 0,
        item: ItemTarget::from_digest([2; 32]),
    };
    for rows in [
        vec![],
        vec![row, row],
        vec![row; 33],
        vec![SelectedSupply {
            cost_ordinal: 8,
            ..row
        }],
    ] {
        let mut invalid = state.clone();
        let Action::UseAbility { supplies, .. } = &mut invalid
            .observation
            .interactions
            .as_mut()
            .unwrap()
            .preparation
            .as_mut()
            .unwrap()
            .action
        else {
            panic!("ability");
        };
        *supplies = SupplySelection::Explicit { items: rows };
        assert_eq!(invalid.validate(), Err(InvalidState::InvalidInteraction));
    }
}

#[test]
fn paid_choice_and_instance_charges_validate_and_survive_full_delta_replacement() {
    let base = room(5, 5, 1);
    let mut next = base.clone();
    next.revision += 1;
    let item = ItemTarget::from_digest([75; 32]);
    next.observation.inventory.push(ItemView {
        id: item,
        class: ItemClass::Wand,
        quantity: 1,
        name: "ivory wand".into(),
        appearance: "ivory wand".into(),
        identified: false,
        corpse: None,
        description: String::new(),
        asset: None,
        known_currency: None,
        known_supply: None,
    });
    next.observation.interactions = Some(InteractionView {
        curse_blocked: vec![],
        identification: Some(IdentificationChoiceView {
            choice: IntentionId("opaque-paid-choice".into()),
            class: ItemClass::Wand,
            items: vec![item],
        }),
        completed: vec![Action::Read {
            item: ItemTarget::from_digest([76; 32]),
        }],
        slots: vec![],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item,
            slot: None,
            equipped_slot: None,
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: true,
            charges: None,
        }],
    });
    round_trip(&base, &next);
    let mut identified = next.clone();
    identified.revision += 1;
    identified.observation.inventory[0].identified = true;
    let interaction = identified.observation.interactions.as_mut().unwrap();
    interaction.identification = None;
    interaction.inventory[0].charges = Some(0);
    round_trip(&next, &identified);
    for case in 0..5 {
        let mut invalid = next.clone();
        let interaction = invalid.observation.interactions.as_mut().unwrap();
        match case {
            0 => {
                interaction.identification.as_mut().unwrap().items =
                    vec![ItemTarget::from_digest([77; 32])]
            }
            1 => interaction.identification.as_mut().unwrap().class = ItemClass::Potion,
            2 => interaction
                .identification
                .as_mut()
                .unwrap()
                .items
                .push(item),
            3 => interaction.inventory[0].charges = Some(0),
            4 => interaction.completed.push(Action::Identify {
                choice: IntentionId("opaque-paid-choice".into()),
                item: None,
            }),
            _ => unreachable!(),
        }
        assert_eq!(
            invalid.validate(),
            Err(InvalidState::InvalidInteraction),
            "case {case}"
        );
    }
}

#[test]
fn remembered_projectile_bonus_delta_requires_identified_item_and_bounds() {
    let mut base = equipped_state(false);
    let row = &mut base.observation.interactions.as_mut().unwrap().inventory[0];
    row.slot = None;
    row.equipped_slot = None;
    row.known_equipment = None;
    assert!(base.validate().is_ok());
    let mut next = base.clone();
    next.revision += 1;
    next.observation.interactions.as_mut().unwrap().inventory[0].known_projectile_bonus =
        Some(ProjectileBonusView {
            hit: -1_000_000,
            damage: 1_000_000,
        });
    round_trip(&base, &next);
    let mut invalid = next.clone();
    invalid.observation.inventory[0].identified = false;
    assert_eq!(invalid.validate(), Err(InvalidState::InvalidInteraction));
    next.observation.interactions.as_mut().unwrap().inventory[0]
        .known_projectile_bonus
        .as_mut()
        .unwrap()
        .hit = 1_000_001;
    assert_eq!(next.validate(), Err(InvalidState::InvalidInteraction));
}

#[test]
fn full_remembered_projectile_inventory_fits_retained_state_budget() {
    let mut state = equipped_state(false);
    let item = state.observation.inventory[0].clone();
    let mut row = state.observation.interactions.as_ref().unwrap().inventory[0].clone();
    row.slot = None;
    row.equipped_slot = None;
    row.known_equipment = None;
    row.known_projectile_bonus = Some(ProjectileBonusView {
        hit: -1_000_000,
        damage: -1_000_000,
    });
    state.observation.inventory.clear();
    state
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .inventory
        .clear();
    // Equipment memory holds at most 4096 exact-instance facts.
    for index in 0..4096 {
        let target = ItemTarget::from_digest(synthetic_digest(index + 100));
        let mut item = item.clone();
        item.id = target;
        state.observation.inventory.push(item);
        let mut row = row.clone();
        row.item = target;
        state
            .observation
            .interactions
            .as_mut()
            .unwrap()
            .inventory
            .push(row);
    }
    assert!(state.validate().is_ok());
    assert!(encoded_length(&state, MAX_STATE_BYTES).unwrap().is_some());
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<StateView>(&json).unwrap(), state);
}

#[test]
fn remembered_curse_and_protection_do_not_invent_equipment_slots_or_numeric_knowledge() {
    let mut base = equipped_state(false);
    let row = &mut base.observation.interactions.as_mut().unwrap().inventory[0];
    row.slot = None;
    row.equipped_slot = None;
    row.known_equipment = None;
    for identified in [false, true] {
        base.observation.inventory[0].identified = identified;
        for (cursed, protected) in [
            (Some(true), None),
            (None, Some(false)),
            (Some(false), Some(true)),
        ] {
            let mut known = base.clone();
            known.revision += 1;
            let row = &mut known.observation.interactions.as_mut().unwrap().inventory[0];
            row.known_cursed = cursed;
            row.known_protected = protected;
            assert!(row.known_projectile_bonus.is_none());
            assert!(
                known.validate().is_ok(),
                "exact-instance facts need neither slots nor identified type"
            );
            round_trip(&base, &known);
        }
    }
    for equipment in [false, true] {
        let mut invalid = base.clone();
        let row = &mut invalid.observation.interactions.as_mut().unwrap().inventory[0];
        if equipment {
            row.known_equipment = Some(EquipmentView {
                attack: None,
                modifiers: vec![],
            });
        } else {
            row.equipped_slot = Some(0);
        }
        assert_eq!(
            invalid.validate(),
            Err(InvalidState::InvalidInteraction),
            "actual equipment still requires a socket"
        );
    }
}

#[test]
fn anonymous_ability_feedback_survives_validated_snapshot_codec_and_delta() {
    let base = room(5, 5, 1);
    let mut next = room(5, 5, 2);
    next.observation.combat = Some(CombatView {
        own_stats: None,
        public_plan: None,
        hp: 10,
        max_hp: 10,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        sequence: None,
        actors: vec![],
        events: vec![CombatEventView::AnonymousAbility {
            caster: Some(next.observation.self_target),
            target: None,
            outcome: AbilityOutcome::Miss,
        }],
        objective: None,
        exit: None,
        victory: false,
        dead: false,
        terminal: false,
    });
    next.validate().unwrap();
    round_trip(&base, &next);
    let message = complete_observation(next, 2);
    let encoded = encode_bounded_json(&message, MAX_RESPONSE_BYTES).unwrap();
    assert_eq!(decode_response(&encoded).unwrap(), message);
    for surplus in [
        r#""type":"anonymous_ability","ability":{"id":"private","name":"Secret wand"}"#,
        r#""type":"anonymous_ability","source":"private""#,
        r#""type":"anonymous_ability","strike":0"#,
    ] {
        let malformed = encoded.replace(r#""type":"anonymous_ability""#, surplus);
        assert_ne!(malformed, encoded);
        assert!(decode_response(&malformed).is_err());
    }
}
