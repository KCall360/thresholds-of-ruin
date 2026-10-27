use std::sync::Arc;
use tor_client_ascii::{App, Effect, Input, Key};
use tor_client_common::{charts::ChartPages, ClientState};
use tor_protocol::{chart_pages::*, *};

fn app() -> App {
    let mut observation = tor_client_common::test_support::observation();
    observation.charts = vec![ChartSummary {
        supported: true,
        kind: ChartKind::Monsters,
        mode: ChartMode::Live,
        status: ChartStatus::Available,
        acquired_at: Some(0),
        chart: Some("sample".into()),
        latest_attempt: None,
    }];
    observation.visible_cells.push(
        serde_json::from_value(serde_json::json!({"key":"known-stairs",
        "position":{"x":0,"y":0,"z":0},"wall":false,"lit":true,"place_hint":false,
        "stairs_up":false,"stairs_down":true,"door":null,"trap":null,"object_presentation":null}))
        .unwrap(),
    );
    let snapshot = Snapshot {
        context: StreamContext {
            stream: StreamId("chart-test".into()),
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
        actor: observation.actor,
        branch: BranchId("main".into()),
        cursor: StreamCursor {
            sequence: 0,
            tick: 0,
        },
        state: Arc::new(StateView {
            wizard_game: false,
            revision: 0,
            observation,
        }),
        has_control: true,
        history: HistoryPage {
            entries: vec![],
            older_before: None,
        },
    };
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    app
}
fn key(app: &mut App, key: Key) -> Effect {
    app.input(Input::Key { key })
}

#[test]
fn chart_modal_draws_complete_disclosure_but_never_offers_map_actions_and_escape_restores_controls()
{
    // Establish the ordinary controlling action before introducing the modal.
    // A separate same-boundary app avoids a locally pending, unsent action.
    let mut ordinary = app();
    assert_eq!(ordinary.role, AccessRole::Player);
    assert!(ordinary.state.as_ref().unwrap().has_control());
    assert!(ordinary.state.as_ref().unwrap().can_admit_intention());
    let Effect::Request(ordinary_stair_request) = key(&mut ordinary, Key::Descend) else {
        panic!("ordinary stair control must work before opening a chart")
    };
    assert!(matches!(
        *ordinary_stair_request,
        Request::Command {
            command: Command::Act {
                action: Action::Move {
                    direction: Direction::Down
                },
                ..
            },
            ..
        }
    ));
    let mut app = app();
    assert_eq!(
        app.state.as_ref().unwrap().snapshot(),
        ordinary.state.as_ref().unwrap().snapshot()
    );
    let before = app.state.as_ref().unwrap().snapshot();
    let map = tor_client_ascii::render::map_tiles(app.state.as_ref().unwrap(), &app.palette);
    assert_eq!(key(&mut app, Key::Charts), Effect::None);
    assert!(app.charts_open);
    let Effect::Request(request) = key(&mut app, Key::Enter) else {
        panic!("chart query")
    };
    let Request::Chart { query } = *request else {
        panic!("not a read-only chart query")
    };
    let state = app.state.as_ref().unwrap();
    let mut worker = ChartPages::default();
    worker.bind(&query, "chart-test", state).unwrap();
    let page = ChartPage {
        context: query.context.clone(),
        branch: query.branch.clone(),
        base: query.base,
        chart: query.chart.clone(),
        kind: ChartKind::Monsters,
        mode: ChartMode::Live,
        acquired_at: 0,
        total_components: 1,
        total_records: 2,
        start: 0,
        next: None,
        records: vec![
            ChartRecord::Component(ChartComponent {
                component: 0,
                alignment: ChartAlignment::Aligned,
            }),
            ChartRecord::Row(ChartRow {
                component: 0,
                position: Some([5, -2, 9]),
                mark: ChartMark::Monster {
                    name: "painted stairs".into(),
                    asset: None,
                },
            }),
        ],
    };
    let context = state.snapshot().reply_context();
    let message = ServerMessage::ChartPage {
        context: context.clone(),
        request_id: "chart-test".into(),
        page: Box::new(page.clone()),
    };
    worker
        .accept(
            &context,
            "chart-test",
            &page,
            serde_json::to_vec(&message).unwrap().len(),
            state,
        )
        .unwrap();
    app.charts
        .install(worker.presentation().unwrap(), state)
        .unwrap();
    assert!(app
        .chart_lines()
        .join("\n")
        .contains("Panel 1 at (5, -2, 9): painted stairs"));
    let mut open = tor_client_ascii::render::Canvas::default();
    open.draw(&app);
    for pressed in [
        Key::Descend,
        Key::Ascend,
        Key::Attack,
        Key::Pickup,
        Key::Wait,
        Key::Right,
    ] {
        assert_eq!(key(&mut app, pressed), Effect::None);
    }
    assert_eq!(app.input(Input::Click { x: 600, y: 400 }), Effect::None);
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert_eq!(
        tor_client_ascii::render::map_tiles(app.state.as_ref().unwrap(), &app.palette),
        map
    );
    assert_eq!(key(&mut app, Key::Escape), Effect::CancelCharts);
    assert!(!app.charts_open);
    let mut closed = tor_client_ascii::render::Canvas::default();
    closed.draw(&app);
    assert!(
        open.pixels != closed.pixels,
        "chart modal must change rendered pixels"
    );
    let Effect::Request(request) = key(&mut app, Key::Descend) else {
        panic!("ordinary stair control restored")
    };
    assert_eq!(
        request, ordinary_stair_request,
        "the chart cannot create or alter stair authority"
    );
    assert!(matches!(
        *request,
        Request::Command {
            command: Command::Act {
                action: Action::Move {
                    direction: Direction::Down
                },
                ..
            },
            ..
        }
    ));
}

#[test]
fn spectators_can_select_charts_and_a_new_observation_withholds_old_live_rows() {
    let mut app = app();
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    snapshot.has_control = false;
    snapshot.readiness.admission = false;
    app.role = AccessRole::Spectator;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(!app.state.as_ref().unwrap().has_control());
    assert!(!app.state.as_ref().unwrap().can_admit_intention());
    assert_eq!(key(&mut app, Key::Descend), Effect::None);
    assert_eq!(key(&mut app, Key::Charts), Effect::None);
    let Effect::Request(request) = key(&mut app, Key::Enter) else {
        panic!("spectator read-only chart")
    };
    assert!(matches!(*request, Request::Chart { .. }));
    let mut next = app.state.as_ref().unwrap().state().clone();
    next.revision += 1;
    next.observation.charts[0].status = ChartStatus::Inactive;
    next.observation.charts[0].chart = None;
    next.observation.charts[0].acquired_at = None;
    app.update(StreamUpdate {
        context: app.state.as_ref().unwrap().context().clone(),
        actor: next.observation.actor,
        branch: app.state.as_ref().unwrap().branch().clone(),
        cursor: StreamCursor {
            sequence: 1,
            tick: next.observation.tick,
        },
        body: UpdateBody::Observation {
            state: Arc::new(next),
            event: None,
        },
    })
    .unwrap();
    assert!(app.charts.display().is_none());
    assert!(app.chart_lines().join("\n").contains("inactive"));
    assert_eq!(key(&mut app, Key::Escape), Effect::CancelCharts);
}

#[test]
fn successful_snapshot_recovery_cancels_chart_selection_but_invalid_replacement_is_atomic() {
    let mut app = app();
    assert_eq!(key(&mut app, Key::Charts), Effect::None);
    assert!(matches!(key(&mut app, Key::Enter), Effect::Request(_)));
    assert_eq!(app.charts.selected(), Some(ChartKind::Monsters));
    assert!(app.charts.loading());
    let original = app.state.as_ref().unwrap().snapshot();

    // An old epoch is invalid; neither the world nor the pending selection changes.
    assert!(app.replace_snapshot(original.clone()).is_err());
    assert_eq!(app.state.as_ref().unwrap().snapshot(), original);
    assert_eq!(app.charts.selected(), Some(ChartKind::Monsters));
    assert!(app.charts.loading());

    let mut replacement = original;
    replacement.context.epoch += 1;
    app.replace_snapshot(replacement.clone()).unwrap();
    assert!(app.charts_open);
    assert_eq!(app.charts.selected(), None);
    assert!(!app.charts.loading());
    assert!(app.charts.display().is_none());
    let Effect::Request(request) = key(&mut app, Key::Enter) else {
        panic!("recovery must allow a new pure chart query from the catalog")
    };
    let Request::Chart { query } = *request else {
        panic!("chart selection cannot become an action")
    };
    assert_eq!(query.context, replacement.context);
    assert_eq!(query.cursor, None);
}

#[test]
fn replacing_transport_owner_returns_to_chart_catalog_instead_of_reusing_pending_selection() {
    let mut app = app();
    assert_eq!(key(&mut app, Key::Charts), Effect::None);
    assert!(matches!(key(&mut app, Key::Enter), Effect::Request(_)));
    assert!(app.charts.loading());
    let replacement = ClientState::from_snapshot(app.state.as_ref().unwrap().snapshot()).unwrap();
    app.set_state(replacement);
    assert_eq!(app.charts.selected(), None);
    assert!(!app.charts.loading());
    assert!(matches!(key(&mut app, Key::Enter), Effect::Request(_)));
}

#[test]
fn chart_timeout_returns_to_catalog_without_actions_and_old_view_timeout_is_ignored() {
    let mut app = app();
    assert_eq!(key(&mut app, Key::Charts), Effect::None);
    let Effect::Request(request) = key(&mut app, Key::Enter) else {
        panic!("chart query")
    };
    let Request::Chart { query } = *request else {
        panic!("pure chart query")
    };
    let before = app.state.as_ref().unwrap().snapshot();
    let mut stale = query.clone();
    stale.context.epoch += 1;
    app.chart_cancelled(&stale);
    assert!(app.charts.loading());
    assert_eq!(app.charts.selected(), Some(ChartKind::Monsters));
    app.chart_cancelled(&query);
    assert!(app.charts_open);
    assert!(!app.charts.loading());
    assert_eq!(app.charts.selected(), None);
    assert!(app.status.contains("timed out"));
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert!(
        matches!(key(&mut app, Key::Enter), Effect::Request(request) if matches!(*request, Request::Chart { .. }))
    );
}

#[test]
fn mapping_modal_renders_sparse_historical_stairs_and_wall_normal_without_map_or_operation_authority(
) {
    let mut app = app();
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
    observation.visible_cells.clear();
    observation.charts[0].kind = ChartKind::Mapping;
    observation.charts[0].mode = ChartMode::Historical;
    observation.charts[0].supported = false;
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    let tiles = tor_client_ascii::render::map_tiles(app.state.as_ref().unwrap(), &app.palette);
    assert_eq!(key(&mut app, Key::Charts), Effect::None);
    let Effect::Request(request) = key(&mut app, Key::Enter) else {
        panic!("mapping query")
    };
    let Request::Chart { query } = *request else {
        panic!("read-only mapping query")
    };
    let state = app.state.as_ref().unwrap();
    let mut worker = ChartPages::default();
    worker.bind(&query, "mapping", state).unwrap();
    let page = ChartPage {
        context: query.context.clone(),
        branch: query.branch.clone(),
        base: query.base,
        chart: query.chart.clone(),
        kind: ChartKind::Mapping,
        mode: ChartMode::Historical,
        acquired_at: 0,
        total_components: 1,
        total_records: 4,
        start: 0,
        next: None,
        records: vec![
            ChartRecord::Component(ChartComponent {
                component: 0,
                alignment: ChartAlignment::Aligned,
            }),
            ChartRecord::Row(ChartRow {
                component: 0,
                position: Some([0, 0, 0]),
                mark: ChartMark::Mapping {
                    fact: MappingMark::WallFace {
                        normal: Some([0, -1, 0]),
                    },
                },
            }),
            ChartRecord::Row(ChartRow {
                component: 0,
                position: Some([0, 0, 0]),
                mark: ChartMark::Mapping {
                    fact: MappingMark::Stairs,
                },
            }),
            ChartRecord::Row(ChartRow {
                component: 0,
                position: Some([0, 0, 0]),
                mark: ChartMark::Mapping {
                    fact: MappingMark::TrapPresence,
                },
            }),
        ],
    };
    let context = state.snapshot().reply_context();
    let message = ServerMessage::ChartPage {
        context: context.clone(),
        request_id: "mapping".into(),
        page: Box::new(page.clone()),
    };
    worker
        .accept(
            &context,
            "mapping",
            &page,
            serde_json::to_vec(&message).unwrap().len(),
            state,
        )
        .unwrap();
    app.charts
        .install(worker.presentation().unwrap(), state)
        .unwrap();
    let text = app.chart_lines().join("\n");
    for label in [
        "Mapping: historical",
        "Unsupported in the current scope",
        "wall face (normal 0, -1, 0)",
        "stairs presence",
        "trap presence",
    ] {
        assert!(text.contains(label), "{text}");
    }
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    for pressed in [
        Key::Descend,
        Key::Ascend,
        Key::Attack,
        Key::Pickup,
        Key::Wait,
    ] {
        assert_eq!(key(&mut app, pressed), Effect::None);
    }
    assert_eq!(app.state.as_ref().unwrap().snapshot(), snapshot);
    assert_eq!(
        tor_client_ascii::render::map_tiles(app.state.as_ref().unwrap(), &app.palette),
        tiles
    );
    assert_eq!(key(&mut app, Key::Escape), Effect::CancelCharts);
    assert!(app
        .state
        .as_ref()
        .unwrap()
        .state()
        .observation
        .visible_cells
        .is_empty());
}
