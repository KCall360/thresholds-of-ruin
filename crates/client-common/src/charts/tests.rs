use super::*;

fn client(kind: ChartKind, mode: ChartMode, status: ChartStatus) -> ClientState {
    let summary = ChartSummary {
        kind,
        mode,
        status,
        acquired_at: Some(4),
        chart: Some("sample".into()),
        latest_attempt: Some(ChartAttempt {
            attempted_at: 8,
            status: ChartAttemptStatus::Empty,
        }),
    };
    let observation = Observation {
        charts: vec![summary],
        self_appearance: None,
        interactions: None,
        combat: None,
        motion: None,
        places: vec![],
        actor: ActorId(1),
        self_target: ActorTarget::from_digest([1; 32]),
        linked_holds: vec![],
        tick: 10,
        position: Position { x: 0, y: 0, z: 0 },
        visible_cells: vec![],
        ground_items: vec![],
        inventory: vec![],
        visible_actors: vec![],
        ready: true,
    };
    ClientState::from_snapshot(Snapshot {
        context: StreamContext {
            stream: StreamId("charts".into()),
            epoch: 0,
        },
        readiness: Readiness {
            revision: 0,
            admission: true,
            resume: vec![],
            cancel: vec![],
        },
        intentions: vec![],
        travel: None,
        actor: ActorId(1),
        branch: BranchId("main".into()),
        cursor: StreamCursor {
            sequence: 0,
            tick: 10,
        },
        state: Arc::new(StateView {
            wizard_game: false,
            revision: 2,
            observation,
        }),
        has_control: true,
        history: HistoryPage {
            entries: vec![],
            older_before: None,
        },
    })
    .unwrap()
}
fn component(id: u64, alignment: ChartAlignment) -> ChartRecord {
    ChartRecord::Component(ChartComponent {
        component: id,
        alignment,
    })
}
fn monster(id: u64, position: Option<[i64; 3]>, name: &str) -> ChartRecord {
    ChartRecord::Row(ChartRow {
        component: id,
        position,
        mark: ChartMark::Monster {
            name: name.into(),
            asset: None,
        },
    })
}
fn page(
    client: &ClientState,
    query: &ChartQuery,
    records: Vec<ChartRecord>,
    components: u64,
    total: u64,
) -> ChartPage {
    let summary = &client.state().observation.charts[0];
    let start = query.cursor.unwrap_or(0);
    let end = start + records.len() as u64;
    ChartPage {
        context: query.context.clone(),
        branch: query.branch.clone(),
        base: query.base,
        chart: query.chart.clone(),
        kind: summary.kind,
        mode: summary.mode,
        acquired_at: summary.acquired_at.unwrap(),
        total_components: components,
        total_records: total,
        start,
        records,
        next: (end < total).then_some(end),
    }
}
fn accept(
    pages: &mut ChartPages,
    state: &ClientState,
    records: Vec<ChartRecord>,
    components: u64,
    total: u64,
    id: &str,
) -> Result<bool, String> {
    let query = pages.next_query().unwrap();
    pages.bind(&query, id, state).unwrap();
    let page = page(state, &query, records, components, total);
    let context = state.snapshot().reply_context();
    let wire = ServerMessage::ChartPage {
        context: context.clone(),
        request_id: id.into(),
        page: Box::new(page.clone()),
    };
    let bytes = encode_response(&wire, None, MAX_RESPONSE_BYTES)
        .unwrap()
        .text
        .len();
    pages.accept(&context, id, &page, bytes, state)
}

#[test]
fn whole_chart_hydration_is_atomic_preserves_repeated_occurrences_and_never_changes_world_view() {
    let state = client(ChartKind::Monsters, ChartMode::Live, ChartStatus::Available);
    let before = state.snapshot();
    let memory: Vec<_> = state.map_memory().map(|cell| cell.appearance()).collect();
    let mut pages = ChartPages::default();
    assert!(matches!(
        pages
            .request(ChartInput::Select(ChartKind::Monsters), &state)
            .unwrap(),
        Some(Request::Chart { .. })
    ));
    let mut records: Vec<_> = (0..700)
        .map(|id| component(id, ChartAlignment::Aligned))
        .collect();
    records.extend((0..800).map(|_| monster(699, Some([12, -3, 9]), "stairs")));
    for (number, chunk) in records.chunks(300).enumerate() {
        let complete = accept(
            &mut pages,
            &state,
            chunk.to_vec(),
            700,
            1500,
            &format!("q{number}"),
        )
        .unwrap();
        assert_eq!(complete, number == 4);
        if number < 4 {
            assert!(pages.display().is_none());
            assert!(pages.lines(&state).iter().any(|l| l.contains("Loading")));
        }
    }
    let display = pages.display().unwrap();
    assert_eq!(display.components.len(), 700);
    assert_eq!(display.rows.len(), 800);
    assert!(display.rows.windows(2).all(|rows| rows[0] == rows[1]));
    assert!(pages
        .lines(&state)
        .iter()
        .any(|l| l.contains("Panel 700 at (12, -3, 9): stairs")));
    assert_eq!(state.snapshot(), before);
    assert_eq!(
        state
            .map_memory()
            .map(|cell| cell.appearance())
            .collect::<Vec<_>>(),
        memory
    );
    assert!(state.state().observation.visible_actors.is_empty());
}

#[test]
fn historical_capture_and_latest_attempt_are_distinct_and_neutral_names_do_not_infer_identity() {
    let state = client(
        ChartKind::Monsters,
        ChartMode::Historical,
        ChartStatus::Available,
    );
    let mut pages = ChartPages::default();
    pages
        .request(ChartInput::Select(ChartKind::Monsters), &state)
        .unwrap();
    accept(
        &mut pages,
        &state,
        vec![
            component(0, ChartAlignment::Aligned),
            component(1, ChartAlignment::Inconsistent),
            monster(0, Some([i64::MIN, i64::MAX, -9]), ""),
            monster(1, None, ""),
        ],
        2,
        4,
        "q",
    )
    .unwrap();
    let lines = pages.lines(&state).join("\n");
    assert!(lines.contains("historical, available, captured at tick 4"));
    assert!(lines.contains("Latest attempt at tick 8: empty"));
    assert!(lines.contains("(-9223372036854775808, 9223372036854775807, -9): creature"));
    assert!(lines.contains("position unavailable (inconsistent joins): creature"));
    for hidden in [
        "sentinel",
        "actor:",
        "item:",
        "sample",
        "species",
        "stairs_down",
    ] {
        assert!(!lines.contains(hidden));
    }
}

#[test]
fn revision_history_context_and_owner_changes_withhold_old_live_rows_and_partial_pages() {
    let state = client(ChartKind::Monsters, ChartMode::Live, ChartStatus::Available);
    let mut pages = ChartPages::default();
    pages
        .request(ChartInput::Select(ChartKind::Monsters), &state)
        .unwrap();
    accept(
        &mut pages,
        &state,
        vec![
            component(0, ChartAlignment::Aligned),
            monster(0, Some([1, 2, 3]), "bat"),
        ],
        1,
        2,
        "q",
    )
    .unwrap();
    let presentation = pages.presentation().unwrap();
    for change in 0..4 {
        let mut next = state.snapshot();
        match change {
            0 => {
                Arc::make_mut(&mut next.state).revision += 1;
                next.cursor.sequence += 1;
            }
            1 => next.context.epoch += 1,
            2 => next.branch.0.push('x'),
            3 => {
                next.actor = ActorId(2);
                Arc::make_mut(&mut next.state).observation.actor = ActorId(2);
            }
            _ => unreachable!(),
        }
        let next = ClientState::from_snapshot(next).unwrap();
        let mut changed = pages.clone();
        changed.observe(&next);
        assert!(changed.display().is_none());
        assert!(!changed.lines(&next).join("\n").contains("bat"));
        assert!(changed.install(presentation.clone(), &next).is_err());
    }
    let mut next = state.snapshot();
    next.cursor.sequence += 1;
    Arc::make_mut(&mut next.state).revision += 1;
    let summary = &mut Arc::make_mut(&mut next.state).observation.charts[0];
    summary.status = ChartStatus::Inactive;
    summary.acquired_at = None;
    summary.chart = None;
    let next = ClientState::from_snapshot(next).unwrap();
    pages.observe(&next);
    assert!(pages.display().is_none());
    assert!(!pages.loading());
    assert!(pages.next_query().is_none());
}

#[test]
fn malformed_or_over_budget_completion_does_not_publish_prefix_or_rewrite_server_status() {
    let state = client(ChartKind::Monsters, ChartMode::Live, ChartStatus::Available);
    let mut pages = ChartPages::default();
    pages
        .request(ChartInput::Select(ChartKind::Monsters), &state)
        .unwrap();
    accept(
        &mut pages,
        &state,
        vec![component(0, ChartAlignment::Inconsistent)],
        1,
        2,
        "first",
    )
    .unwrap();
    assert!(pages.display().is_none());
    assert!(accept(
        &mut pages,
        &state,
        vec![monster(0, Some([0, 0, 0]), "bat")],
        1,
        2,
        "last"
    )
    .is_err());
    assert!(pages.display().is_none());
    assert_eq!(
        state.state().observation.charts[0].status,
        ChartStatus::Available
    );
    let mut small = ChartPages::with_budget(ChartAssemblyBudget {
        encoded_bytes: 1,
        ..canonical_budget()
    });
    small
        .request(ChartInput::Select(ChartKind::Monsters), &state)
        .unwrap();
    assert!(accept(
        &mut small,
        &state,
        vec![
            component(0, ChartAlignment::Aligned),
            monster(0, Some([0, 0, 0]), "bat")
        ],
        1,
        2,
        "q"
    )
    .is_err());
    assert!(small.display().is_none());
    assert_eq!(
        state.state().observation.charts[0].status,
        ChartStatus::Available
    );
}

#[test]
fn empty_samples_are_complete_and_local_paging_cancel_never_make_an_action() {
    let state = client(ChartKind::Food, ChartMode::Historical, ChartStatus::Empty);
    let mut pages = ChartPages::default();
    pages
        .request(ChartInput::Select(ChartKind::Food), &state)
        .unwrap();
    assert!(accept(
        &mut pages,
        &state,
        vec![component(0, ChartAlignment::Aligned)],
        1,
        1,
        "q"
    )
    .unwrap());
    assert!(pages.display().unwrap().rows.is_empty());
    for input in [
        ChartInput::Next,
        ChartInput::Previous,
        ChartInput::Cancel,
        ChartInput::Overview,
    ] {
        assert_eq!(pages.request(input, &state).unwrap(), None);
    }
    assert!(pages.display().is_none());
    assert!(pages.next_query().is_none());
    assert_eq!(
        parse("monsters"),
        Ok(ChartInput::Select(ChartKind::Monsters))
    );
    for input in [
        "north",
        "attack bat",
        "stairs",
        "take food",
        "magic and north",
    ] {
        assert!(parse(input).is_err());
    }
}
