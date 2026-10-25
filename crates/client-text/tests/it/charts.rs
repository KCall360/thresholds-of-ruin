use std::{collections::VecDeque, sync::Arc, time::Duration};
use tor_client_common::{
    charts::{ChartInput, ChartPages},
    ClientState, Palette,
};
use tor_client_text::{
    engine::{turn::Link, Engine, Outcome},
    Input,
};
use tor_protocol::{chart_pages::*, *};

struct ChartLink {
    role: AccessRole,
    state: ClientState,
    palette: Palette,
    charts: ChartPages,
    sent: Vec<Request>,
    queue: VecDeque<ServerMessage>,
}
impl ChartLink {
    fn new() -> Self {
        let mut observation = tor_client_common::test_support::observation();
        observation.charts = vec![ChartSummary {
            kind: ChartKind::Monsters,
            mode: ChartMode::Live,
            status: ChartStatus::Available,
            acquired_at: Some(0),
            chart: Some("sample".into()),
            latest_attempt: None,
        }];
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
        Self {
            role: AccessRole::Spectator,
            state: ClientState::from_snapshot(snapshot).unwrap(),
            palette: Palette::default(),
            charts: ChartPages::default(),
            sent: Vec::new(),
            queue: VecDeque::new(),
        }
    }
    fn enqueue(
        &mut self,
        request: Request,
    ) -> Result<String, tor_client_text::engine::turn::Error> {
        let Request::Chart { query } = &request else {
            panic!("chart caller sent gameplay: {request:?}")
        };
        let id = format!("chart-{}", self.sent.len());
        self.charts.bind(query, &id, &self.state)?;
        let records = if query.cursor.is_none() {
            vec![ChartRecord::Component(ChartComponent {
                component: 0,
                alignment: ChartAlignment::Aligned,
            })]
        } else {
            vec![
                ChartRecord::Row(ChartRow {
                    component: 0,
                    position: Some([3, 4, 5]),
                    mark: ChartMark::Monster {
                        name: "painted stairs".into(),
                        asset: None
                    }
                });
                2
            ]
        };
        let first = query.cursor.is_none();
        let page = ChartPage {
            context: query.context.clone(),
            branch: query.branch.clone(),
            base: query.base,
            chart: query.chart.clone(),
            kind: ChartKind::Monsters,
            mode: ChartMode::Live,
            acquired_at: 0,
            total_components: 1,
            total_records: 3,
            start: query.cursor.unwrap_or(0),
            records,
            next: first.then_some(1),
        };
        self.queue.push_back(ServerMessage::ChartPage {
            context: self.state.snapshot().reply_context(),
            request_id: id.clone(),
            page: Box::new(page),
        });
        self.sent.push(request);
        Ok(id)
    }
}
impl Link for ChartLink {
    fn client(&self) -> &ClientState {
        &self.state
    }
    fn palette(&self) -> &Palette {
        &self.palette
    }
    fn role(&self) -> AccessRole {
        self.role
    }
    fn is_synchronized(&self) -> bool {
        true
    }
    fn set_pace(&mut self, _: Duration) {}
    fn pace(&self) -> Duration {
        Duration::ZERO
    }
    fn chart_request(&mut self, input: ChartInput) -> Result<Option<Request>, String> {
        self.charts.request(input, &self.state)
    }
    fn chart_lines(&self) -> Vec<String> {
        self.charts.lines(&self.state)
    }
    fn chart_loading(&self) -> bool {
        self.charts.loading()
    }
    fn cancel_chart(&mut self) {
        let _ = self.charts.request(ChartInput::Cancel, &self.state);
    }
    async fn send(
        &mut self,
        request: Request,
    ) -> Result<String, tor_client_text::engine::turn::Error> {
        self.enqueue(request)
    }
    async fn next(
        &mut self,
        _: Duration,
    ) -> Result<Option<ServerMessage>, tor_client_text::engine::turn::Error> {
        let Some(message) = self.queue.pop_front() else {
            return Ok(None);
        };
        if let ServerMessage::ChartPage {
            context,
            request_id,
            page,
        } = &message
        {
            let bytes = encode_response(&message, None, MAX_RESPONSE_BYTES)?
                .text
                .len();
            self.charts
                .accept(context, request_id, page, bytes, &self.state)?;
            if let Some(query) = self.charts.next_query() {
                self.enqueue(Request::Chart { query })?;
            }
        }
        Ok(Some(message))
    }
}

#[tokio::test]
async fn ordinary_adventure_chart_command_pages_without_actions_or_new_scenery_referents() {
    let mut link = ChartLink::new();
    let before = link.state.snapshot();
    assert_eq!(
        tor_client_text::parse("chart monsters", link.state.state()),
        Ok(Input::Chart(ChartInput::Select(ChartKind::Monsters)))
    );
    let mut engine = Engine::default();
    let Outcome::Passage(text) = engine.play(&mut link, "chart monsters").await.unwrap() else {
        panic!("chart quit")
    };
    assert!(text.contains("live, available, captured at tick 0"));
    assert_eq!(
        text.matches("Panel 1 at (3, 4, 5): painted stairs").count(),
        2
    );
    assert_eq!(link.sent.len(), 2);
    assert!(link.sent.iter().all(|r| matches!(r, Request::Chart { .. })));
    assert_eq!(link.state.snapshot(), before);
    let scene = tor_client_text::engine::scene::Scene::new(link.state.state(), &link.palette);
    assert!(scene.referents.iter().all(|r| r.name != "painted stairs"));
    link.role = AccessRole::Player;
    let _ = engine
        .play(&mut link, "attack painted stairs")
        .await
        .unwrap();
    assert_eq!(link.sent.len(), 2);
    for line in ["chart next", "chart previous", "chart cancel", "charts"] {
        let _ = engine.play(&mut link, line).await.unwrap();
    }
    assert_eq!(link.sent.len(), 2);
    assert_eq!(link.state.snapshot(), before);
}

#[test]
fn explicit_chart_parser_never_reinterprets_invalid_chart_input_as_actions() {
    let link = ChartLink::new();
    assert_eq!(
        tor_client_text::parse("charts", link.state.state()),
        Ok(Input::Chart(ChartInput::Overview))
    );
    for line in [
        "chart north",
        "chart attack sentinel",
        "chart food and north",
        "chart stairs",
    ] {
        assert!(tor_client_text::parse(line, link.state.state()).is_err());
    }
}
