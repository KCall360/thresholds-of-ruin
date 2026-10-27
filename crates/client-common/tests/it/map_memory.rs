use std::sync::Arc;
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(cells: &[(&str, i32, i32)], revision: u64) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":false,"resume":[],"cancel":[]},"context":super::stream_context(0),
        "actor":"1","branch":"map","cursor":{"sequence":"0","tick":revision.to_string()},
        "has_control":true, "intentions":[],"history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":revision.to_string(),"observation":{
            "actor":"1","self_target":tor_protocol::ActorTarget::from_digest([1; 32]),"tick":revision.to_string(),"position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":cells.iter().map(|(key,x,y)| serde_json::json!({
                "key":key,"position":{"x":x,"y":y,"z":0},"wall":false,
                "trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,"lit":true,"place_hint":false
            })).collect::<Vec<_>>(),
            "ground_items":[],"inventory":[],"charts":[],"linked_holds":[],"visible_actors":[],"ready":true
        }}
    }))
    .unwrap()
}

fn advance(client: &mut ClientState, next: Snapshot) {
    client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.actor,
            branch: next.branch,
            cursor: StreamCursor {
                sequence: client.cursor().sequence + 1,
                tick: next.cursor.tick,
            },
            body: UpdateBody::Observation {
                state: next.state,
                event: None,
            },
        })
        .unwrap();
}

#[test]
fn narration_rejects_gaps_atomically_and_resets_on_snapshot() {
    let mut client = ClientState::from_snapshot(snapshot(&[("a", 0, 0)], 0)).unwrap();
    let mut seen = snapshot(&[("a", 0, 0)], 1);
    Arc::make_mut(&mut seen.state)
        .observation
        .visible_actors
        .push(ActorView {
            appearance_seed: None,
            asset: None,
            id: tor_protocol::ActorTarget::from_digest([2; 32]),
            name: "figure".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
    advance(&mut client, seen);
    assert_eq!(client.narration(), ["You notice a figure."]);
    let unchanged = client.clone();
    let mut next = snapshot(&[("a", 0, 0)], 2);
    assert!(client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.actor,
            branch: next.branch.clone(),
            cursor: StreamCursor {
                sequence: 3,
                tick: 2
            },
            body: UpdateBody::Observation {
                state: next.state.clone(),
                event: None
            },
        })
        .is_err());
    assert_eq!(client, unchanged);
    super::reset_snapshot(&mut client, next.clone()).unwrap();
    assert!(client.narration().is_empty());
    next.branch = BranchId("rewound".into());
    super::reset_snapshot(&mut client, next).unwrap();
    assert!(client.narration().is_empty());
}

#[test]
fn map_aligns_every_update_and_refreshes_items_without_retaining_actors() {
    let mut initial = snapshot(&[("a", 0, 0), ("b", 1, 0), ("item", 3, 0)], 0);
    Arc::make_mut(&mut initial.state)
        .observation
        .ground_items
        .push(GroundItemView {
            presentation: None,
            item: ItemView {
                corpse: None,
                class: Default::default(),
                asset: None,
                quantity: 1,
                appearance: String::new(),
                identified: true,
                id: tor_protocol::ItemTarget::from_digest([7; 32]),
                name: "token".into(),
                description: String::new(),
                known_currency: None,
                known_supply: None,
            },
            position: Position { x: 3, y: 0, z: 0 },
            reachable: false,
            supply_reachable: false,
        });
    Arc::make_mut(&mut initial.state)
        .observation
        .visible_actors
        .push(ActorView {
            appearance_seed: None,
            asset: None,
            id: tor_protocol::ActorTarget::from_digest([2; 32]),
            position: Position { x: 3, y: 0, z: 0 },
            name: "figure".into(),
            description: String::new(),
        });
    let mut client = ClientState::from_snapshot(initial).unwrap();
    advance(&mut client, snapshot(&[("a", -1, 0), ("b", 0, 0)], 1));
    advance(&mut client, snapshot(&[("b", -1, 0), ("c", 0, 0)], 2));
    let remembered = client.map_memory().find(|c| c.key == "item").unwrap();
    assert_eq!(remembered.position, Position { x: 1, y: 0, z: 0 });
    assert_eq!(remembered.ground_items.len(), 1);
    let serialized = serde_json::to_value(remembered.appearance()).unwrap();
    assert!(serialized.get("visible_actors").is_none());
    assert!(serialized["ground_items"][0].get("id").is_none());
    // The historical memory contract remains unchanged.
    assert_eq!(
        client
            .memory()
            .find(|c| c.key == "item")
            .unwrap()
            .position
            .x,
        3
    );
    advance(&mut client, snapshot(&[("c", 0, 0), ("item", 1, 0)], 3));
    assert!(client
        .map_memory()
        .find(|c| c.key == "item")
        .unwrap()
        .ground_items
        .is_empty());
}

#[test]
fn snapshots_preserve_aligned_map_but_rewind_and_unalignable_views_reset_it() {
    let mut client =
        ClientState::from_snapshot(snapshot(&[("a", 0, 0), ("old", 1, 0)], 0)).unwrap();
    super::reset_snapshot(&mut client, snapshot(&[("a", 0, 0)], 1)).unwrap();
    assert_eq!(client.map_memory().count(), 2);
    super::reset_snapshot(&mut client, snapshot(&[("elsewhere", 0, 0)], 2)).unwrap();
    assert_eq!(client.map_memory().count(), 1);
    let mut rewind = snapshot(&[("a", 0, 0)], 0);
    rewind.branch = BranchId("new".into());
    super::reset_snapshot(&mut client, rewind).unwrap();
    assert_eq!(client.map_memory().count(), 1);
    assert_eq!(client.map_memory().next().unwrap().key, "a");
}

#[test]
fn conflicting_or_ambiguous_occurrences_do_not_guess_a_global_map() {
    let mut client =
        ClientState::from_snapshot(snapshot(&[("a", 0, 0), ("b", 1, 0), ("old", 2, 0)], 0))
            .unwrap();
    advance(&mut client, snapshot(&[("a", 0, 0), ("b", 0, 1)], 1));
    assert!(client.map_memory().all(|c| c.key != "old"));
    let mut client =
        ClientState::from_snapshot(snapshot(&[("a", 0, 0), ("a", 3, 0), ("old", 2, 0)], 0))
            .unwrap();
    advance(&mut client, snapshot(&[("a", 0, 0), ("a", 3, 0)], 1));
    assert!(client.map_memory().all(|c| c.key != "old"));
}

#[test]
fn map_tracks_elevation_and_rejects_bad_updates_atomically() {
    let first = snapshot(&[("anchor", 0, 0), ("old", 1, 0)], 0);
    let mut client = ClientState::from_snapshot(first).unwrap();
    let mut upstairs = snapshot(&[("anchor", 0, 0)], 1);
    Arc::make_mut(&mut upstairs.state).observation.visible_cells[0]
        .position
        .z = -1;
    advance(&mut client, upstairs);
    assert_eq!(
        client
            .map_memory()
            .find(|c| c.key == "old")
            .unwrap()
            .position
            .z,
        -1
    );
    let before = client.clone();
    let next = snapshot(&[("anchor", 0, 0)], 2);
    assert!(client
        .apply(StreamUpdate {
            context: client.context().clone(),
            actor: next.actor,
            branch: next.branch,
            cursor: StreamCursor {
                sequence: 99,
                tick: 2
            },
            body: UpdateBody::Observation {
                state: next.state,
                event: None
            }
        })
        .is_err());
    assert_eq!(client, before);
}

#[test]
fn chart_size_and_extreme_translations_are_bounded() {
    let mut initial = snapshot(&[("anchor", 0, 0)], 0);
    let template = initial.state.observation.visible_cells[0].clone();
    for x in 1..5000 {
        let mut cell = template.clone();
        cell.key = format!("cell-{x}");
        cell.position.x = x;
        Arc::make_mut(&mut initial.state)
            .observation
            .visible_cells
            .push(cell);
    }
    let mut client = ClientState::from_snapshot(initial).unwrap();
    assert_eq!(client.map_memory().count(), 32);
    advance(&mut client, snapshot(&[("anchor", i32::MAX, 0)], 1));
    assert_eq!(client.map_memory().count(), 0);
}

fn cells(row: &[(String, i32, i32)]) -> Vec<(&str, i32, i32)> {
    row.iter().map(|(k, x, y)| (k.as_str(), *x, *y)).collect()
}

fn delta_update(client: &ClientState, delta: StateDelta) -> StreamUpdate {
    StreamUpdate {
        context: client.context().clone(),
        actor: ActorId(1),
        branch: client.branch().clone(),
        cursor: StreamCursor {
            sequence: client.cursor().sequence + 1,
            tick: delta.tick,
        },
        body: UpdateBody::ObservationDelta {
            base: client.observation_base(),
            state: Box::new(delta),
            event: None,
        },
    }
}

#[test]
fn deltas_rebuild_the_full_view_and_reject_another_base_atomically() {
    let row = |ox: i32| -> Vec<(String, i32, i32)> {
        (0..8).map(|x| (format!("{x}"), x - ox, 0)).collect()
    };
    let first = row(0);
    let mut client = ClientState::from_snapshot(snapshot(&cells(&first), 0)).unwrap();
    // One step east: every retained cell moves one place west.
    let mut next = Arc::unwrap_or_clone(snapshot(&cells(&row(1)[1..]), 1).state);
    next.observation.visible_cells[0].wall = true;
    let delta = StateDelta::between(client.state(), &next).unwrap();
    assert_eq!(delta.cells.shift, Position { x: -1, y: 0, z: 0 });
    client.apply(delta_update(&client, delta)).unwrap();
    assert_eq!(client.state(), &next);

    let unchanged = client.clone();
    let stale = StateDelta::between(&snapshot(&cells(&first), 0).state, &next);
    let mut stale = stale.unwrap();
    stale.revision = 2;
    stale.tick = 2;
    assert_eq!(
        client.apply(delta_update(&client, stale)),
        Err(tor_client_common::StreamError::InconsistentState)
    );
    assert_eq!(client, unchanged);
}

#[test]
fn overflowing_delta_preserves_the_entire_client_model() {
    for (coordinate, shift) in [(i32::MAX, 1), (i32::MIN, -1)] {
        let row: Vec<_> = (0..8)
            .map(|y| (format!("edge-{y}"), coordinate, y))
            .collect();
        let initial = snapshot(&cells(&row), 0);
        let mut client = ClientState::from_snapshot(initial.clone()).unwrap();
        let mut next = Arc::unwrap_or_clone(initial.state);
        next.revision = 1;
        next.observation.tick = 1;
        let mut delta = StateDelta::between(client.state(), &next).unwrap();
        delta.cells.shift.x = shift;
        let unchanged = client.clone();
        assert_eq!(
            client.apply(delta_update(&client, delta)),
            Err(tor_client_common::StreamError::InconsistentState)
        );
        assert_eq!(client, unchanged);
        super::reset_snapshot(&mut client, snapshot(&[("recovered", 0, 0)], 1)).unwrap();
        assert_eq!(client.state().revision, 1);
    }
}

#[test]
fn known_map_lookup_preserves_opaque_memory_keys_and_prefers_current_terrain() {
    let target = Position { x: 1, y: 0, z: 0 };
    let mut client =
        ClientState::from_snapshot(snapshot(&[("here", 0, 0), ("opaque-remembered", 1, 0)], 0))
            .unwrap();
    advance(&mut client, snapshot(&[("here", 0, 0)], 1));
    assert_eq!(client.map_cell(target).unwrap().key, "opaque-remembered");
    assert!(client.map_cell(Position { z: 1, ..target }).is_none());
    assert!(client.map_cell(Position { x: 99, ..target }).is_none());
    let mut now = snapshot(&[("here", 0, 0), ("fresh-wall", 1, 0)], 2);
    Arc::make_mut(&mut now.state).observation.visible_cells[1].wall = true;
    advance(&mut client, now);
    let cell = client.map_cell(target).unwrap();
    assert_eq!(cell.key, "fresh-wall");
    assert!(cell.wall);
    advance(&mut client, snapshot(&[("here", 0, 0)], 3));
    assert!(client.map_cell(target).unwrap().wall);
}

fn map_cell(position: Position, key: &str) -> RememberedMapCell {
    RememberedMapCell {
        object_presentation: None,
        key: key.into(),
        position,
        wall: false,
        material: "stone".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: vec![],
    }
}
fn map_page(
    client: &ClientState,
    cells: Vec<RememberedMapCell>,
    total: usize,
    start: usize,
    page: usize,
) -> MapPage {
    let end = start + cells.len();
    MapPage {
        transfer: "map-transfer".into(),
        actor: client.state().observation.actor,
        branch: client.branch().clone(),
        base: client.observation_base(),
        window: MapWindow::normal(),
        start: start as u32,
        page: page as u16,
        total_cells: total as u32,
        complete: true,
        ambiguous: false,
        limited: false,
        cells,
        next: (end < total).then(|| MapPageCursor {
            transfer: "map-transfer".into(),
            next: end as u32,
            page: page as u16 + 1,
        }),
    }
}
fn bind_map(client: &mut ClientState, id: &str) -> MapQuery {
    let query = client.next_map_query().expect("one automatic query");
    client.bind_map_query(&query, id).unwrap();
    query
}
fn accept_map(client: &mut ClientState, id: &str, page: &MapPage) -> Result<(), String> {
    let context = client.reply_context();
    let message = ServerMessage::MapPage {
        context: context.clone(),
        request_id: id.into(),
        page: Box::new(page.clone()),
    };
    let bytes = encoded_length(&message, MAX_MAP_PAGE_BYTES)
        .unwrap()
        .unwrap();
    client.accept_map_page(&context, id, page, bytes)
}

#[test]
fn full_six_plane_hydration_is_atomic_bounded_and_does_not_gate_live_input() {
    let mut client = ClientState::from_snapshot(snapshot(&[("here", 0, 0)], 0)).unwrap();
    let live = client.state().clone();
    let readiness = client.readiness().clone();
    let mut cells = Vec::new();
    for x in -31..=31 {
        for y in -16..=16 {
            for z in -2..=3 {
                cells.push(map_cell(Position { x, y, z }, &format!("{x}:{y}:{z}")));
            }
        }
    }
    assert_eq!(cells.len(), MAX_MAP_TRANSFER_CELLS);
    for (index, chunk) in cells.chunks(MAX_MAP_PAGE_CELLS).enumerate() {
        let id = format!("page-{index}");
        let query = bind_map(&mut client, &id);
        assert_eq!(
            query.cursor.as_ref().map(|c| c.page),
            (index > 0).then_some(index as u16)
        );
        assert!(
            client.next_map_query().is_none(),
            "redraw cannot duplicate an in-flight request"
        );
        let page = map_page(
            &client,
            chunk.to_vec(),
            cells.len(),
            index * MAX_MAP_PAGE_CELLS,
            index,
        );
        accept_map(&mut client, &id, &page).unwrap();
        if page.next.is_some() {
            assert_eq!(
                client.map_memory().count(),
                1,
                "no partial staging can become display state"
            );
            assert!(client.map_hydration().pending);
        }
        assert_eq!(client.state(), &live);
        assert_eq!(client.readiness(), &readiness);
    }
    assert_eq!(client.map_memory().count(), MAX_MAP_TRANSFER_CELLS);
    assert!(client.memory().count() <= MAX_MAP_TRANSFER_CELLS);
    assert_eq!(client.map_hydration().pages, 25);
    assert!(client.map_hydration().complete);
    assert!(!client.map_hydration().pending);
    assert!(client.map_hydration().bytes <= MAX_MAP_TRANSFER_BYTES);
    assert!(
        client.next_map_query().is_none(),
        "finished window must not loop"
    );
    assert_eq!(
        client.map_cell(Position { x: 0, y: 0, z: 0 }).unwrap().key,
        "here",
        "live wins"
    );
    assert_eq!(
        client
            .memory()
            .find(|c| c.key == "31:16:3")
            .unwrap()
            .last_seen_revision,
        None
    );
}

#[test]
fn map_pages_reject_context_actor_branch_base_and_sequence_mismatches_without_publication() {
    let mut client = ClientState::from_snapshot(snapshot(&[("here", 0, 0)], 0)).unwrap();
    bind_map(&mut client, "first");
    let first = map_page(
        &client,
        vec![map_cell(Position { x: 1, y: 0, z: 0 }, "old")],
        2,
        0,
        0,
    );
    let before = client.clone();
    let mut wrong = first.clone();
    wrong.actor = ActorId(99);
    assert!(accept_map(&mut client, "first", &wrong).is_err());
    let mut wrong = first.clone();
    wrong.branch = BranchId("another".into());
    assert!(accept_map(&mut client, "first", &wrong).is_err());
    let mut wrong = first.clone();
    wrong.base.revision += 1;
    assert!(accept_map(&mut client, "first", &wrong).is_err());
    let mut context = client.reply_context();
    context.input.stream.epoch += 1;
    assert!(client
        .accept_map_page(&context, "first", &first, 1000)
        .is_err());
    assert!(accept_map(&mut client, "unsolicited", &first).is_err());
    let mut wrong = first.clone();
    wrong.start = 1;
    wrong.page = 1;
    assert!(accept_map(&mut client, "first", &wrong).is_err());
    assert_eq!(client, before);
    accept_map(&mut client, "first", &first).unwrap();
    assert!(
        accept_map(&mut client, "first", &first).is_err(),
        "duplicate cannot advance continuation"
    );
    bind_map(&mut client, "second");
    let duplicated = map_page(&client, first.cells.clone(), 2, 1, 1);
    assert!(accept_map(&mut client, "second", &duplicated).is_err());
    assert_eq!(client.map_memory().count(), 1);
}

#[test]
fn map_aliases_keep_all_occurrences_but_reject_conflicting_appearances() {
    for conflict in [false, true] {
        let mut client = ClientState::from_snapshot(snapshot(&[("here", 0, 0)], 0)).unwrap();
        bind_map(&mut client, "aliases");
        let first = map_cell(Position { x: 1, y: 0, z: 0 }, "same-physical-cell");
        let mut second = first.clone();
        second.position.x = 2;
        second.wall = conflict;
        let first_page = map_page(&client, vec![first], 2, 0, 0);
        accept_map(&mut client, "aliases", &first_page).unwrap();
        bind_map(&mut client, "alias-next");
        let page = map_page(&client, vec![second], 2, 1, 1);
        let result = accept_map(&mut client, "alias-next", &page);
        assert_eq!(result.is_err(), conflict);
        if conflict {
            assert_eq!(client.map_memory().count(), 1);
        } else {
            assert_eq!(
                client
                    .map_memory()
                    .filter(|c| c.key == "same-physical-cell")
                    .count(),
                2
            );
            assert_eq!(
                client
                    .memory()
                    .filter(|c| c.key == "same-physical-cell")
                    .count(),
                1
            );
        }
    }
}

#[test]
fn newer_observations_cancel_staging_and_limited_results_keep_an_aligned_cache() {
    let mut client =
        ClientState::from_snapshot(snapshot(&[("here", 0, 0), ("old", 1, 0)], 0)).unwrap();
    let query = bind_map(&mut client, "old-query");
    let page = map_page(
        &client,
        vec![map_cell(Position { x: 2, y: 0, z: 0 }, "staged")],
        2,
        0,
        0,
    );
    accept_map(&mut client, "old-query", &page).unwrap();
    advance(&mut client, snapshot(&[("here", 0, 0)], 1));
    assert!(!client.map_query_pending("old-query", &query));
    assert!(accept_map(&mut client, "old-query", &page).is_err());
    bind_map(&mut client, "limited");
    let mut page = map_page(&client, vec![], 0, 0, 0);
    page.complete = false;
    page.limited = true;
    accept_map(&mut client, "limited", &page).unwrap();
    assert!(client.map_cell(Position { x: 1, y: 0, z: 0 }).is_some());
    assert!(client.map_hydration().limited);
    assert!(!client.map_hydration().complete);
    assert!(
        client.next_map_query().is_none(),
        "finished limited query never retries the same base/window"
    );
    let mut reset = snapshot(&[("here", 0, 0)], 1);
    reset.context.epoch = client.context().epoch + 1;
    client.replace_snapshot(reset).unwrap();
    assert!(
        client.next_map_query().is_some(),
        "attachment reset requests fresh hydration"
    );
}

#[test]
fn map_hydration_queries_only_initial_unaligned_or_uncovered_windows() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    bind_map(&mut client, "initial");
    let page = map_page(&client, vec![], 0, 0, 0);
    accept_map(&mut client, "initial", &page).unwrap();
    advance(&mut client, oriented_snapshot(0, 0, 0, 1));
    assert!(
        client.next_map_query().is_none(),
        "unchanged aligned coverage stays valid"
    );
    advance(&mut client, oriented_snapshot(-1, 0, 0, 2));
    assert!(
        client.next_map_query().is_some(),
        "translation exits one edge of certified coverage"
    );
    advance(&mut client, snapshot(&[("unrelated", 0, 0)], 3));
    assert!(
        client.next_map_query().is_some(),
        "unrelated scene needs its own chart"
    );
}

#[test]
fn map_transfer_counts_complete_reply_bytes_and_preserves_live_view_on_budget_failure() {
    let mut client = ClientState::from_snapshot(snapshot(&[("here", 0, 0)], 0)).unwrap();
    for index in 0..64 {
        let id = format!("bytes-{index}");
        bind_map(&mut client, &id);
        let page = map_page(
            &client,
            vec![map_cell(
                Position {
                    x: (index % 63) as i32 - 31,
                    y: (index / 63) as i32,
                    z: 0,
                },
                &format!("c-{index}"),
            )],
            65,
            index,
            index,
        );
        client
            .accept_map_page(&client.reply_context(), &id, &page, MAX_MAP_PAGE_BYTES)
            .unwrap();
    }
    assert!(client.next_map_query().is_none());
    assert!(client.map_hydration().failed && !client.map_hydration().pending);
    assert_eq!(client.map_memory().count(), 1);
    assert_eq!(client.map_hydration().bytes, MAX_MAP_TRANSFER_BYTES);
}

#[test]
fn completed_ambiguous_map_publishes_certified_cells_and_failure_does_not_loop() {
    let mut client = ClientState::from_snapshot(snapshot(&[("here", 0, 0)], 0)).unwrap();
    bind_map(&mut client, "ambiguous");
    let mut page = map_page(
        &client,
        vec![map_cell(Position { x: 1, y: 0, z: 0 }, "certified")],
        1,
        0,
        0,
    );
    page.ambiguous = true;
    accept_map(&mut client, "ambiguous", &page).unwrap();
    assert!(client.map_hydration().complete && client.map_hydration().ambiguous);
    assert!(!client.map_hydration().limited);
    assert_eq!(
        client.map_cell(Position { x: 1, y: 0, z: 0 }).unwrap().key,
        "certified"
    );
    advance(&mut client, snapshot(&[("elsewhere", 0, 0)], 1));
    bind_map(&mut client, "refused");
    assert!(client.reject_map_query("refused"));
    assert!(client.map_hydration().failed);
    assert!(client.next_map_query().is_none());
    advance(&mut client, snapshot(&[("elsewhere", 0, 0)], 2));
    assert!(
        client.next_map_query().is_none(),
        "unchanged views do not retry a completed failure"
    );
}

#[test]
fn clipped_coverage_does_not_claim_evicted_cells_after_reversing_direction() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    bind_map(&mut client, "initial");
    let page = map_page(
        &client,
        vec![map_cell(Position { x: -31, y: 0, z: 0 }, "left-edge")],
        1,
        0,
        0,
    );
    accept_map(&mut client, "initial", &page).unwrap();
    advance(&mut client, oriented_snapshot(-1, 0, 0, 1));
    let query = bind_map(&mut client, "limited");
    let mut limited = map_page(&client, vec![], 0, 0, 0);
    limited.window = query.window;
    limited.complete = false;
    limited.limited = true;
    accept_map(&mut client, "limited", &limited).unwrap();
    assert!(client.map_memory().all(|c| c.key != "left-edge"));
    advance(&mut client, oriented_snapshot(0, 0, 0, 2));
    assert!(
        client.next_map_query().is_some(),
        "reverse travel cannot claim cropped-away coverage"
    );
}

fn oriented_snapshot(dx: i32, dy: i32, dz: i32, revision: u64) -> Snapshot {
    let mut value = snapshot(
        &[("a", dx, dy), ("b", dx + 1, dy), ("c", dx, dy + 1)],
        revision,
    );
    for cell in &mut Arc::make_mut(&mut value.state).observation.visible_cells {
        cell.position.z = dz;
    }
    value
}
fn finish_region(client: &mut ClientState, id: &str, cells: Vec<RememberedMapCell>) -> MapWindow {
    let query = bind_map(client, id);
    let mut page = map_page(client, cells.clone(), cells.len(), 0, 0);
    page.window = query.window;
    accept_map(client, id, &page).unwrap();
    query.window
}

#[test]
fn incremental_map_one_axis_moves_request_only_uncovered_face() {
    for (dx, dy, dz, volume) in [(-1, 0, 0, 198), (0, -1, 0, 378), (0, 0, -1, 2079)] {
        let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
        finish_region(&mut client, "initial", vec![]);
        advance(&mut client, oriented_snapshot(dx, dy, dz, 1));
        let query = client.next_map_query().unwrap();
        assert_eq!(
            query.window.volume(),
            Some(volume),
            "must fetch only the uncovered face"
        );
    }
}

#[test]
fn incremental_map_diagonal_regions_preserve_committed_memory_and_native_partial_coverage() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    finish_region(
        &mut client,
        "initial",
        vec![map_cell(Position { x: 10, y: 10, z: 0 }, "retained")],
    );
    advance(&mut client, oriented_snapshot(-1, -1, 0, 1));
    let mut shown = client.clone();
    let x = finish_region(
        &mut client,
        "x-edge",
        vec![map_cell(Position { x: 31, y: 0, z: 0 }, "edge")],
    );
    assert_eq!(x.volume(), Some(192));
    assert!(
        !client.map_hydration().complete,
        "one edge does not cover the other edge"
    );
    shown
        .apply_map_memory_update(client.map_memory_update().unwrap())
        .unwrap();
    assert_eq!(
        shown.map_cell(Position { x: 31, y: 0, z: 0 }).unwrap().key,
        "edge"
    );
    assert_eq!(
        shown.map_cell(Position { x: 9, y: 9, z: 0 }).unwrap().key,
        "retained"
    );
    let y = finish_region(&mut client, "y-edge", vec![]);
    assert_eq!(y.volume(), Some(378));
    assert!(!x.contains(y.min));
    assert!(client.map_hydration().complete);
    assert!(client.next_map_query().is_none());
}

#[test]
fn incremental_map_three_axis_faces_cover_corners_without_overlap() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    finish_region(&mut client, "initial", vec![]);
    advance(&mut client, oriented_snapshot(-1, -1, -1, 1));
    let mut requested = std::collections::BTreeSet::new();
    for (index, volume) in [160, 315, 2079].into_iter().enumerate() {
        let window = finish_region(&mut client, &format!("face-{index}"), vec![]);
        assert_eq!(window.volume(), Some(volume));
        for x in window.min.x..=window.max.x {
            for y in window.min.y..=window.max.y {
                for z in window.min.z..=window.max.z {
                    assert!(
                        requested.insert(Position { x, y, z }),
                        "faces must not overlap"
                    );
                }
            }
        }
        assert_eq!(client.map_hydration().complete, index == 2);
        assert_eq!(client.map_hydration().pages, index + 1);
    }
    assert_eq!(requested.len(), 2554);
    assert!(requested.contains(&Position { x: 31, y: 16, z: 3 }));
    assert!(client.next_map_query().is_none());
}

#[test]
fn incremental_map_sparse_collinear_and_rotated_anchors_require_full_refresh() {
    for cells in [
        vec![("a", -1, 0)],
        vec![("a", -1, 0), ("b", 0, 0)],
        vec![("a", 0, 0), ("b", 0, 1), ("c", -1, 0)],
    ] {
        let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
        finish_region(&mut client, "initial", vec![]);
        advance(&mut client, snapshot(&cells, 1));
        assert_eq!(client.next_map_query().unwrap().window, MapWindow::normal());
    }
}

#[test]
fn incremental_map_interrupted_initial_pages_never_publish_and_then_settle() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    for step in 0..3 {
        let id = format!("initial-{step}");
        bind_map(&mut client, &id);
        let staged = map_page(
            &client,
            vec![map_cell(Position { x: 10, y: 0, z: 0 }, "never-published")],
            2,
            0,
            0,
        );
        accept_map(&mut client, &id, &staged).unwrap();
        assert!(client.map_hydration().bytes > 0);
        assert_eq!(client.map_hydration().pages, 1);
        advance(
            &mut client,
            oriented_snapshot(-step - 1, 0, 0, (step + 1) as u64),
        );
        assert!(client.map_memory().all(|c| c.key != "never-published"));
        assert!(accept_map(&mut client, &id, &staged).is_err());
    }
    finish_region(
        &mut client,
        "settled",
        vec![map_cell(Position { x: 10, y: 0, z: 0 }, "committed")],
    );
    assert!(client.map_hydration().complete);
    advance(&mut client, oriented_snapshot(-4, 0, 0, 4));
    let query = bind_map(&mut client, "partial-edge");
    let mut page = map_page(
        &client,
        vec![map_cell(Position { x: 31, y: 0, z: 0 }, "staged-edge")],
        2,
        0,
        0,
    );
    page.window = query.window;
    accept_map(&mut client, "partial-edge", &page).unwrap();
    advance(&mut client, oriented_snapshot(-5, 0, 0, 5));
    assert!(client.map_memory().any(|c| c.key == "committed"));
    assert!(client.map_memory().all(|c| c.key != "staged-edge"));
    assert_eq!(client.next_map_query().unwrap().window.volume(), Some(396));
}

#[test]
fn incremental_map_same_base_budget_includes_previous_completed_regions() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    finish_region(&mut client, "initial", vec![]);
    advance(&mut client, oriented_snapshot(-1, -1, -1, 1));
    for index in 0..64 {
        let id = format!("budget-{index}");
        let query = bind_map(&mut client, &id);
        let (start, page, total) = if index == 0 {
            (0, 0, 1)
        } else {
            (index - 1, index - 1, 65)
        };
        let cells = vec![map_cell(
            Position {
                x: if index == 0 { 31 } else { index as i32 - 32 },
                y: if index == 0 { 0 } else { 16 },
                z: 0,
            },
            &id,
        )];
        let mut page = map_page(&client, cells, total, start, page);
        page.window = query.window;
        client
            .accept_map_page(&client.reply_context(), &id, &page, MAX_MAP_PAGE_BYTES)
            .unwrap();
    }
    assert!(client.next_map_query().is_none());
    assert!(client.map_hydration().failed && !client.map_hydration().pending);
    assert_eq!(client.map_hydration().bytes, MAX_MAP_TRANSFER_BYTES);
    assert!(client.map_memory().any(|c| c.key == "budget-0"));
    assert!(client.map_memory().all(|c| c.key != "budget-1"));
    assert!(!client.map_hydration().complete);
}

#[test]
fn incremental_map_ambiguous_region_survives_later_failure_and_current_sight_wins() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    finish_region(
        &mut client,
        "initial",
        vec![
            map_cell(Position { x: 10, y: 10, z: 0 }, "same"),
            map_cell(Position { x: 11, y: 10, z: 0 }, "same"),
        ],
    );
    let mut next = oriented_snapshot(-1, -1, 0, 1);
    Arc::make_mut(&mut next.state)
        .observation
        .visible_cells
        .push(CellView {
            object_presentation: None,
            key: "live-edge".into(),
            position: Position { x: 31, y: 0, z: 0 },
            wall: false,
            lit: true,
            material: String::new(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            trap: None,
            door: None,
        });
    advance(&mut client, next);
    let query = bind_map(&mut client, "ambiguous-edge");
    let mut page = map_page(
        &client,
        vec![map_cell(Position { x: 31, y: 0, z: 0 }, "stale-edge")],
        1,
        0,
        0,
    );
    page.window = query.window;
    page.ambiguous = true;
    accept_map(&mut client, "ambiguous-edge", &page).unwrap();
    assert!(client.map_hydration().ambiguous);
    assert!(!client.map_hydration().complete);
    assert_eq!(
        client.map_cell(Position { x: 31, y: 0, z: 0 }).unwrap().key,
        "live-edge"
    );
    assert_eq!(client.map_memory().filter(|c| c.key == "same").count(), 2);
    let mut shown = client.clone();
    bind_map(&mut client, "failed-second");
    assert!(client.reject_map_query("failed-second"));
    assert!(client.next_map_query().is_none());
    assert!(client.map_hydration().ambiguous && client.map_hydration().failed);
    shown
        .apply_map_memory_update(client.map_memory_update().unwrap())
        .unwrap();
    assert_eq!(
        shown.map_cell(Position { x: 31, y: 0, z: 0 }).unwrap().key,
        "live-edge"
    );
    assert_eq!(shown.map_memory().filter(|c| c.key == "same").count(), 2);
    assert!(!shown.map_hydration().complete);
}

#[test]
fn incremental_map_same_base_alias_conflicts_do_not_rewrite_committed_regions() {
    for conflict in [false, true] {
        let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
        finish_region(&mut client, "initial", vec![]);
        advance(&mut client, oriented_snapshot(-1, -1, 0, 1));
        finish_region(
            &mut client,
            "first",
            vec![map_cell(Position { x: 31, y: 0, z: 0 }, "shared")],
        );
        let query = bind_map(&mut client, "second");
        let mut cell = map_cell(Position { x: 0, y: 16, z: 0 }, "shared");
        cell.wall = conflict;
        let mut page = map_page(&client, vec![cell], 1, 0, 0);
        page.window = query.window;
        assert_eq!(accept_map(&mut client, "second", &page).is_err(), conflict);
        assert!(
            !client
                .map_cell(Position { x: 31, y: 0, z: 0 })
                .unwrap()
                .wall,
            "second region cannot mutate an earlier conflicting alias"
        );
        assert_eq!(
            client.map_memory().filter(|c| c.key == "shared").count(),
            if conflict { 1 } else { 2 }
        );
    }
}

#[test]
fn incremental_map_reserves_reply_headroom_before_requesting_another_page() {
    let mut client = ClientState::from_snapshot(oriented_snapshot(0, 0, 0, 0)).unwrap();
    finish_region(&mut client, "initial", vec![]);
    advance(&mut client, oriented_snapshot(-1, -1, 0, 1));
    for index in 0..64 {
        let id = format!("reserve-{index}");
        let query = bind_map(&mut client, &id);
        let (start, page, total) = if index == 0 {
            (0, 0, 1)
        } else {
            (index - 1, index - 1, 65)
        };
        let cells = vec![map_cell(
            Position {
                x: if index == 0 { 31 } else { index as i32 - 32 },
                y: if index == 0 { 0 } else { 16 },
                z: 0,
            },
            &id,
        )];
        let mut page = map_page(&client, cells, total, start, page);
        page.window = query.window;
        let bytes = if index == 0 {
            MAX_MAP_PAGE_BYTES / 2
        } else {
            MAX_MAP_PAGE_BYTES
        };
        client
            .accept_map_page(&client.reply_context(), &id, &page, bytes)
            .unwrap();
    }
    assert!(
        client.next_map_query().is_none(),
        "do not ask a conforming server for a reply without its maximum byte headroom"
    );
    assert!(client.map_hydration().failed && !client.map_hydration().pending);
    assert_eq!(
        client.map_hydration().bytes,
        MAX_MAP_TRANSFER_BYTES - MAX_MAP_PAGE_BYTES / 2
    );
    assert!(client.map_memory().any(|c| c.key == "reserve-0"));
    assert!(client.map_memory().all(|c| c.key != "reserve-1"));
}

#[test]
fn trap_memory_keeps_only_last_disclosed_family_and_never_an_action_reference() {
    let mut initial = snapshot(&[("a", 0, 0), ("trap", 1, 0)], 0);
    let target = TrapTarget::from_digest([63; 32]);
    Arc::make_mut(&mut initial.state).observation.visible_cells[1].trap = Some(TrapView {
        id: target,
        family: None,
        reachable: true,
    });
    let mut client = ClientState::from_snapshot(initial).unwrap();
    let mut examined = snapshot(&[("a", 0, 0), ("trap", 1, 0)], 1);
    Arc::make_mut(&mut examined.state).observation.visible_cells[1].trap = Some(TrapView {
        id: target,
        family: Some(TrapFamily::SleepingGas),
        reachable: true,
    });
    advance(&mut client, examined);
    assert_eq!(client.narration(), ["You recognize a sleeping gas trap."]);
    advance(&mut client, snapshot(&[("a", 0, 0)], 2));
    assert!(client
        .state()
        .observation
        .visible_cells
        .iter()
        .all(|cell| cell.trap.is_none()));
    let remembered = client
        .map_memory()
        .find(|cell| cell.key == "trap")
        .unwrap()
        .appearance();
    assert_eq!(
        remembered.trap.as_ref().unwrap().family,
        Some(TrapFamily::SleepingGas)
    );
    let encoded = serde_json::to_value(remembered).unwrap();
    assert_eq!(
        encoded["trap"],
        serde_json::json!({"family":"sleeping_gas"})
    );
    assert!(!encoded.to_string().contains(&target.to_string()));
}

#[test]
fn sampled_cosmetics_survive_acquisition_hydration_and_stale_memory_without_authority() {
    let item_presentation = ObjectPresentation {
        name: "stairs".into(),
        asset: None,
        kind: ObjectPresentationKind::Stairs,
    };
    let stair_presentation = ObjectPresentation {
        name: "silver flask".into(),
        asset: None,
        kind: ObjectPresentationKind::Item {
            class: ItemClass::Potion,
        },
    };
    let mut initial = snapshot(
        &[("a", 0, 0), ("b", 1, 0), ("c", 0, 1), ("object", 3, 0)],
        0,
    );
    let o = &mut Arc::make_mut(&mut initial.state).observation;
    o.visible_cells[3].stairs_down = true;
    o.visible_cells[3].object_presentation = Some(stair_presentation.clone());
    o.ground_items.push(GroundItemView {
        presentation: Some(item_presentation.clone()),
        reachable: false,
        supply_reachable: false,
        position: Position { x: 3, y: 0, z: 0 },
        item: ItemView {
            corpse: None,
            class: ItemClass::Coin,
            quantity: 3,
            appearance: "object".into(),
            identified: true,
            description: String::new(),
            id: ItemTarget::from_digest([67; 32]),
            name: "object".into(),
            asset: None,
            known_supply: None,
            known_currency: Some(CurrencyKind::Gold),
        },
    });
    let mut client = ClientState::from_snapshot(initial).unwrap();
    let assert_saved = |client: &ClientState| {
        let saved = client
            .map_memory()
            .find(|cell| cell.key == "object")
            .unwrap();
        assert_eq!(saved.object_presentation, Some(stair_presentation.clone()));
        assert_eq!(
            saved.items()[0].presentation,
            Some(item_presentation.clone())
        );
        assert_eq!(saved.items()[0].class, ItemClass::Coin);
        assert_eq!(saved.items()[0].quantity, 3);
        assert!(saved.stairs_down);
        assert_eq!(
            saved.appearance().object_presentation,
            Some(stair_presentation.clone())
        );
    };
    assert_saved(&client);
    advance(
        &mut client,
        snapshot(&[("a", 0, 0), ("b", 1, 0), ("c", 0, 1)], 1),
    );
    assert_saved(&client);
    let before = client.clone();
    let row = client
        .map_memory()
        .find(|cell| cell.key == "object")
        .unwrap()
        .appearance();
    let encoded = serde_json::to_value(&row).unwrap();
    let decoded: RememberedMapCell = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, row);
    assert_eq!(client, before);
    let mut hydrated = ClientState::from_snapshot(snapshot(&[("a", 0, 0)], 0)).unwrap();
    bind_map(&mut hydrated, "cosmetics");
    let page = map_page(&hydrated, vec![decoded], 1, 0, 0);
    accept_map(&mut hydrated, "cosmetics", &page).unwrap();
    assert_saved(&hydrated);
}

#[test]
fn typed_cosmetic_projection_is_validated_before_client_publication() {
    let mut initial = snapshot(&[("a", 0, 0)], 0);
    let cell = &mut Arc::make_mut(&mut initial.state).observation.visible_cells[0];
    cell.stairs_down = true;
    cell.object_presentation = Some(ObjectPresentation {
        name: "stairs".into(),
        asset: None,
        kind: ObjectPresentationKind::Stairs,
    });
    assert!(initial.state.validate().is_ok());
    let mut invalid = initial;
    Arc::make_mut(&mut invalid.state).observation.visible_cells[0]
        .object_presentation
        .as_mut()
        .unwrap()
        .name = "x".repeat(257);
    assert_eq!(
        invalid.state.validate(),
        Err(InvalidState::InvalidPresentation)
    );
    assert!(ClientState::from_snapshot(invalid).is_err());
}
