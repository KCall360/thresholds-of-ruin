use std::{
    future::Future,
    net::SocketAddr,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::Duration,
};
use tokio::{sync::mpsc as async_mpsc, time::timeout};
use tor_client_common::{server_before_local, Connection, FirstReady, Palette};
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;
pub enum Event {
    Palette(Palette),
    Role(AccessRole),
    Snapshot(Box<Snapshot>),
    Update(Box<StreamUpdate>),
    MapMemory(Box<tor_client_common::MapMemoryUpdate>),
    ChartPresentation(tor_client_common::charts::ChartPresentation),
    ChartCancelled(chart_pages::ChartQuery),
    Status(String),
    /// The server refused a request, in the player's words.
    Refused(String),
    Ready,
    History(HistoryPage),
    CreatureInspection(Box<CreatureInspectionView>),
    CreatureCatalog(Box<CreatureCatalogPage>),
    ProjectileChoicesSent(String),
    ProjectileChoices {
        context: ReplyContext,
        request_id: String,
        page: Box<ProjectileChoicePage>,
    },
    WeaponPreviewSent(String),
    AbilityWeaponPreview {
        context: ReplyContext,
        request_id: String,
        preview: Box<AbilityWeaponPreview>,
    },
    StrikeQuerySent(String),
    AdvancementQuerySent(String),
    AdvancementPage {
        context: ReplyContext,
        request_id: String,
        page: Box<AdvancementPage>,
    },
    CreatureStrikePage {
        context: ReplyContext,
        request_id: String,
        page: Box<CreatureStrikePage>,
    },
    CombatDiagnostics(Box<CombatDiagnosticsView>),
    Fatal(String),
}

/// What the window asks of the network worker.
pub enum Command {
    CancelCharts,
    Request {
        context: StreamContext,
        request: Box<Request>,
    },
    /// Show what has arrived without waiting.
    Skip,
    /// Space shown updates this far apart.
    Pace(Duration),
}

pub struct Network {
    pub commands: async_mpsc::Sender<Command>,
    pub events: Receiver<Event>,
    worker: JoinHandle<Result<(), String>>,
}

impl Network {
    pub fn start(
        address: SocketAddr,
        token: String,
        actor: ActorId,
        observe: bool,
        pace: Duration,
    ) -> Self {
        let (commands, rx) = async_mpsc::channel(4);
        let (tx, events) = mpsc::sync_channel(64);
        let worker = std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| -> Error { e.into() })
                .and_then(|runtime| {
                    runtime.block_on(run(address, token, actor, observe, pace, rx, &tx))
                });
            if let Err(error) = &result {
                let _=tx.try_send(Event::Fatal(format!("Connection ended: {error}. Relaunch to reconnect; inspect history before retrying.")));
            }
            result.map_err(|error| error.to_string())
        });
        Self {
            commands,
            events,
            worker,
        }
    }

    pub fn shutdown(self) -> Result<(), Error> {
        drop(self.commands);
        // Drain presentation while the worker performs save-and-quit.
        for _ in self.events {}
        self.worker
            .join()
            .map_err(|_| "Network worker failed")?
            .map_err(Into::into)
    }
}

// Backpressure stays on this dedicated worker, never the native event loop.
// The server retains its bounded slow-client disconnect/snapshot-on-relaunch
// contract. Do not drop intermediate observations or grow this queue.
fn publish(tx: &SyncSender<Event>, event: Event) -> Result<(), Error> {
    tx.send(event)
        .map_err(|_| "Window stopped consuming network updates".into())
}

/// Apply an already-delivered server boundary before considering a queued
/// native command. The command retains its original revision and context.
async fn next_worker_input(
    server: impl Future<Output = Result<ServerMessage, Error>>,
    command: impl Future<Output = Option<Command>>,
) -> FirstReady<Result<ServerMessage, Error>, Option<Command>> {
    server_before_local(server, command).await
}

async fn run(
    address: SocketAddr,
    token: String,
    actor: ActorId,
    observe: bool,
    pace: Duration,
    mut rx: async_mpsc::Receiver<Command>,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let mut connection = Connection::connect(address, token, actor, "ascii").await?;
    connection.use_external_advancement_presentation();
    connection.set_pace(pace);
    publish(tx, Event::Role(connection.role()))?;
    let mut shown_palette = (None, false);
    publish_palette_change(&mut shown_palette, &connection.palette, tx)?;
    publish(tx, Event::Snapshot(Box::new(connection.state.snapshot())))?;
    if connection.role() == AccessRole::Spectator {
        publish(
            tx,
            Event::Status("Spectator access is read-only. F2: history; Esc: close.".into()),
        )?;
    } else if !observe {
        transact(
            &mut connection,
            &mut shown_palette,
            Request::AcquireControl,
            tx,
        )
        .await?;
    } else {
        publish(
            tx,
            Event::Status("Observing. Press F3 to request control.".into()),
        )?;
    }
    publish(tx, Event::Ready)?;
    loop {
        match next_worker_input(next(&mut connection, &mut shown_palette, tx), rx.recv()).await {
            FirstReady::Server(message) => {
                let message = message?;
                let reset = matches!(message, ServerMessage::Snapshot { .. });
                present(message, tx)?;
                if reset {
                    publish(tx, Event::Ready)?;
                }
            }
            FirstReady::Local(command) => match command {
                None => {
                    connection.close().await?;
                    return Ok(());
                }
                Some(Command::Request { context, request }) => {
                    if !connection.is_synchronized() {
                        publish(
                            tx,
                            Event::Status("Resynchronizing; request was not sent.".into()),
                        )?;
                        continue;
                    }
                    let stale_input = matches!(request.as_ref(), Request::Command { context, .. }
                        if context != &connection.state.input_context());
                    let stale_action = matches!(request.as_ref(), Request::Command {
                        command: tor_protocol::Command::Act { expected_revision, .. }, ..
                    } if *expected_revision != connection.state.state().revision);
                    if &context != connection.state.context() || stale_input || stale_action {
                        publish(
                            tx,
                            Event::Refused(
                                "State changed; request was not sent. Review the new state.".into(),
                            ),
                        )?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    if matches!(request.as_ref(), Request::Chart { query } if query.base != connection.state.observation_base()
                        || query.context != *connection.state.context() || query.branch != *connection.state.branch())
                    {
                        publish(
                            tx,
                            Event::Refused("Chart view changed; select the chart again.".into()),
                        )?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    // Observation revisions can advance without a new input
                    // generation while the native UI is still presenting old
                    // state. Refuse that queued query before local page tracking
                    // mistakes its stale revision for a fatal transport failure.
                    let stale_advancement = matches!(request.as_ref(), Request::Command {
                        command: tor_protocol::Command::Advancement { expected_revision, .. }, ..
                    } if *expected_revision != connection.state.state().revision);
                    if stale_advancement {
                        publish(tx, Event::Refused("Advancement changed; request was not sent. Request overview and explicitly preview again.".into()))?;
                        publish(tx, Event::Ready)?;
                        continue;
                    }
                    transact(&mut connection, &mut shown_palette, *request, tx).await?;
                    publish(tx, Event::Ready)?;
                }
                Some(Command::CancelCharts) => connection.cancel_charts(),
                Some(Command::Skip) => connection.skip(),
                Some(Command::Pace(pace)) => connection.set_pace(pace),
            },
        }
    }
}

/// Transport owns recovery; the UI gets a copy only when the palette revision
/// or currentness changes. In particular a missed delta redraws the fallback
/// immediately, and a full recovery redraws without an observation update.
async fn next(
    connection: &mut Connection,
    shown: &mut (Option<u64>, bool),
    tx: &SyncSender<Event>,
) -> Result<ServerMessage, Error> {
    next_with_chart_timeout(connection, shown, tx, Duration::from_secs(10)).await
}

async fn next_with_chart_timeout(
    connection: &mut Connection,
    shown: &mut (Option<u64>, bool),
    tx: &SyncSender<Event>,
    chart_timeout: Duration,
) -> Result<ServerMessage, Error> {
    loop {
        // Re-entry also preserves palette invalidations when a command cancels
        // the previous receive future during recovery I/O.
        publish_palette_change(shown, &connection.palette, tx)?;
        let was_loading = connection.charts.loading();
        let cancellation = was_loading
            .then(|| {
                connection
                    .state
                    .state()
                    .observation
                    .charts
                    .iter()
                    .find(|summary| Some(summary.kind) == connection.charts.selected())
                    .and_then(|summary| summary.chart.as_ref())
                    .map(|chart| chart_pages::ChartQuery {
                        context: connection.state.context().clone(),
                        branch: connection.state.branch().clone(),
                        base: connection.state.observation_base(),
                        chart: chart.clone(),
                        cursor: None,
                    })
            })
            .flatten();
        let message = if let Some(query) = cancellation {
            match timeout(chart_timeout, connection.next()).await {
                Ok(result) => result?,
                Err(_) => {
                    // This is local presentation failure, not a fabricated
                    // server reply or a change to the advertised sample status.
                    connection.cancel_charts();
                    publish(tx, Event::ChartCancelled(query))?;
                    continue;
                }
            }
        } else {
            connection.next().await?
        };
        if matches!(&message, ServerMessage::MapPage {page,..} if page.next.is_none()) {
            if let Some(update) = connection.state.map_memory_update() {
                publish(tx, Event::MapMemory(Box::new(update)))?;
            }
        }
        publish_chart_result(was_loading, &connection.charts, &message, tx)?;
        publish_palette_change(shown, &connection.palette, tx)?;
        return Ok(message);
    }
}

fn publish_chart_result(
    was_loading: bool,
    charts: &tor_client_common::charts::ChartPages,
    message: &ServerMessage,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    if (was_loading && !charts.loading())
        || matches!(message, ServerMessage::ChartPage { page, .. } if page.next.is_none())
        || matches!(message, ServerMessage::Error { request_id: Some(id), .. } if id.starts_with("chart-"))
    {
        if let Some(update) = charts.presentation() {
            publish(tx, Event::ChartPresentation(update))?;
        }
    }
    Ok(())
}

fn publish_palette_change(
    shown: &mut (Option<u64>, bool),
    palette: &Palette,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let current = (palette.revision(), palette.stale());
    if *shown != current {
        publish(tx, Event::Palette(palette.clone()))?;
        *shown = current;
    }
    Ok(())
}

async fn transact(
    connection: &mut Connection,
    shown_palette: &mut (Option<u64>, bool),
    request: Request,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let advancement_query = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::Advancement { .. },
            ..
        }
    );
    let strike_query = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::InspectStrikeSource { .. },
            ..
        }
    );
    let projectile_choices = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::ProjectileChoices { .. },
            ..
        }
    );
    let weapon_preview = matches!(
        &request,
        Request::Command {
            command: tor_protocol::Command::PreviewAbilityWeapon { .. },
            ..
        }
    );
    let id = connection.request(request).await?;
    if projectile_choices {
        publish(tx, Event::ProjectileChoicesSent(id.clone()))?;
    }
    if weapon_preview {
        publish(tx, Event::WeaponPreviewSent(id.clone()))?;
    }
    if advancement_query {
        publish(tx, Event::AdvancementQuerySent(id.clone()))?;
    }
    if strike_query {
        publish(tx, Event::StrikeQuerySent(id.clone()))?;
    }
    let mut refresh_preview = weapon_preview || projectile_choices;
    let mut pending = tor_client_common::PendingRequest::new(id);
    timeout(Duration::from_secs(10), async {
        loop {
            let message = next(connection, shown_palette, tx).await?;
            let completion = pending.observe(connection, &message);
            let complete = completion.is_some();
            let unknown = matches!(completion, Some(tor_client_common::RequestCompletion::Unknown));
            let stale_preview = refresh_preview && complete && matches!(&message, ServerMessage::Error {
                code: ErrorCode::StaleRevision | ErrorCode::StaleContext | ErrorCode::WrongBranch, ..
            });
            present(message, tx)?;
            if stale_preview {
                refresh_preview = false;
                let id = connection.request(Request::Snapshot).await?;
                pending = tor_client_common::PendingRequest::new(id);
                publish(tx, Event::Status("Observation changed; refreshing before you choose the exact action again.".into()))?;
                continue;
            }
            if unknown {
                publish(tx, Event::Status("State resynchronized; the previous request may have completed. Inspect history before retrying.".into()))?;
            }
            if complete { return Ok::<(), Error>(()); }
        }
    })
    .await
    .map_err(|_| "Response timed out; the last request may have committed")??;
    Ok(())
}

fn present(message: ServerMessage, tx: &SyncSender<Event>) -> Result<(), Error> {
    match message {
        ServerMessage::ProjectileChoices {
            context,
            request_id,
            page,
        } => publish(
            tx,
            Event::ProjectileChoices {
                context,
                request_id,
                page,
            },
        ),
        ServerMessage::AbilityWeaponPreview {
            context,
            request_id,
            preview,
        } => publish(
            tx,
            Event::AbilityWeaponPreview {
                context,
                request_id,
                preview,
            },
        ),
        ServerMessage::AdvancementPage {
            context,
            request_id,
            page,
        } => publish(
            tx,
            Event::AdvancementPage {
                context,
                request_id,
                page,
            },
        ),
        ServerMessage::Update { update } => publish(tx, Event::Update(update)),
        ServerMessage::Snapshot { snapshot, .. } => publish(tx, Event::Snapshot(snapshot)),
        ServerMessage::Ack {
            receipt: tor_protocol::RequestReceipt::Admitted { phase, .. },
            ..
        } => publish(
            tx,
            Event::Status(tor_client_ascii::phase_status(phase).into()),
        ),
        ServerMessage::Ack {
            receipt: tor_protocol::RequestReceipt::Immediate { .. },
            ..
        } => publish(tx, Event::Status("Done.".into())),
        ServerMessage::Error { code, message, .. } => publish(
            tx,
            Event::Refused(tor_client_ascii::plain_error(code, &message)),
        ),
        ServerMessage::History { page, .. } => {
            publish(tx, Event::History(page))?;
            publish(tx, Event::Status("History loaded.".into()))
        }
        ServerMessage::CreatureCatalog { page, .. } => publish(tx, Event::CreatureCatalog(page)),
        ServerMessage::CombatDiagnostics { report, .. } => {
            publish(tx, Event::CombatDiagnostics(report))
        }
        ServerMessage::CreatureStrikePage {
            context,
            request_id,
            page,
        } => publish(
            tx,
            Event::CreatureStrikePage {
                context,
                request_id,
                page,
            },
        ),
        ServerMessage::CreatureInspection { report, .. } => {
            publish(tx, Event::CreatureInspection(report))
        }
        ServerMessage::Welcome { .. } => Err("Unexpected repeated welcome".into()),
        ServerMessage::SnapshotPart { .. } => Err("Unexpected unassembled snapshot part".into()),
        // `next` already forwarded the current palette, including stale changes.
        ServerMessage::Palette { .. }
        | ServerMessage::MapPage { .. }
        | ServerMessage::ChartPage { .. } => Ok(()),
        // Input permissions arrive through ordered readiness updates.
        ServerMessage::Waiting { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued_run_step() -> Command {
        let stream = StreamContext {
            stream: StreamId("run".into()),
            epoch: 0,
        };
        Command::Request {
            context: stream.clone(),
            request: Box::new(Request::Command {
                context: InputContext {
                    stream,
                    readiness_revision: 0,
                },
                branch: BranchId("main".into()),
                command: tor_protocol::Command::Act {
                    expected_revision: 0,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            }),
        }
    }

    #[tokio::test]
    async fn worker_applies_a_delivered_server_boundary_before_an_already_queued_run_step() {
        let update = StreamUpdate {
            context: StreamContext {
                stream: StreamId("run".into()),
                epoch: 0,
            },
            actor: ActorId(1),
            branch: BranchId("main".into()),
            cursor: StreamCursor {
                sequence: 1,
                tick: 0,
            },
            body: UpdateBody::Readiness {
                readiness: Readiness {
                    revision: 1,
                    admission: true,
                    resume: vec![],
                    cancel: vec![],
                },
            },
        };
        let (commands, mut queued) = async_mpsc::channel(1);
        commands.send(queued_run_step()).await.unwrap();
        let selected = next_worker_input(
            std::future::ready(Ok(ServerMessage::Update {
                update: Box::new(update.clone()),
            })),
            queued.recv(),
        )
        .await;
        assert!(
            matches!(selected, FirstReady::Server(Ok(ServerMessage::Update { update: received })) if *received == update)
        );
        assert!(
            matches!(queued.try_recv(), Ok(Command::Request { .. })),
            "the old command remains queued for the new boundary's stale-input check"
        );
    }

    #[tokio::test]
    async fn worker_accepts_a_queued_command_when_no_server_boundary_is_ready() {
        let (commands, mut queued) = async_mpsc::channel(1);
        commands.send(queued_run_step()).await.unwrap();
        let selected = next_worker_input(std::future::pending(), queued.recv()).await;
        assert!(matches!(
            selected,
            FirstReady::Local(Some(Command::Request { .. }))
        ));
        assert!(queued.try_recv().is_err());
    }

    #[test]
    fn palette_only_changes_reach_the_ui_once_including_stale_and_recovery() {
        let (tx, rx) = mpsc::sync_channel(8);
        let mut palette = Palette::default();
        let mut shown = (None, false);
        let mut changed = |palette: &Palette| {
            publish_palette_change(&mut shown, palette, &tx).unwrap();
            match rx.try_recv().unwrap() {
                Event::Palette(shown) => assert_eq!(&shown, palette),
                _ => panic!("expected palette event"),
            }
            publish_palette_change(&mut shown, palette, &tx).unwrap();
            assert!(rx.try_recv().is_err(), "unchanged palette must not redraw");
        };
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: ["creature.equine.quagga".into()].into(),
            },
        });
        changed(&palette);
        palette.apply(&PaletteUpdate {
            revision: 3,
            body: PaletteBody::Delta {
                base: 2,
                added: Default::default(),
                removed: Default::default(),
            },
        });
        changed(&palette);
        palette.apply(&PaletteUpdate {
            revision: 4,
            body: PaletteBody::Full {
                assets: ["creature.equine.quagga".into()].into(),
            },
        });
        changed(&palette);
    }

    #[test]
    fn palette_publication_survives_mutation_before_reentry_and_failed_delivery() {
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: ["creature.equine.quagga".into()].into(),
            },
        });
        let mut shown = (Some(1), false);
        // Simulate next being cancelled while sending recovery after the
        // palette changed. A fresh per-await baseline would miss this stale
        // transition; the worker's last-published stamp must retain it.
        palette.apply(&PaletteUpdate {
            revision: 3,
            body: PaletteBody::Delta {
                base: 2,
                added: Default::default(),
                removed: Default::default(),
            },
        });
        let (closed, rx) = mpsc::sync_channel(1);
        drop(rx);
        assert!(publish_palette_change(&mut shown, &palette, &closed).is_err());
        assert_eq!(shown, (Some(1), false));
        let (tx, rx) = mpsc::sync_channel(1);
        publish_palette_change(&mut shown, &palette, &tx).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Event::Palette(p)) if p.stale()));
        assert_eq!(shown, (Some(1), true));
        publish_palette_change(&mut shown, &palette, &tx).unwrap();
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn creature_catalog_network_presentation_preserves_the_private_page() {
        let message: ServerMessage = serde_json::from_value(serde_json::json!({
            "type":"creature_catalog", "request_id":"catalog",
            "context":{"input":{"stream":{"stream":"catalog","epoch":"0"},"readiness_revision":"0"},
                "actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"revision":"0"},
            "page":{"depth":27,"entries":[{"selector":"rogue:imp","name":"Imp","details":"Racial growth"}],"next":null}
        })).unwrap();
        let expected = serde_json::to_value(&message).unwrap()["page"].clone();
        let (tx, rx) = mpsc::sync_channel(1);
        present(message, &tx).unwrap();
        let Event::CreatureCatalog(page) = rx.recv().unwrap() else {
            panic!("catalog presentation event");
        };
        assert_eq!(serde_json::to_value(page).unwrap(), expected);
    }

    #[test]
    fn presentation_backpressure_retains_order_and_disconnects_cleanly() {
        let (tx, rx) = mpsc::sync_channel(64);
        for n in 0..64 {
            publish(&tx, Event::Status(n.to_string())).unwrap();
        }
        assert!(matches!(
            tx.try_send(Event::Ready),
            Err(mpsc::TrySendError::Full(_))
        ));
        let worker =
            std::thread::spawn(move || publish(&tx, Event::Ready).map_err(|e| e.to_string()));
        for n in 0..64 {
            assert!(
                matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), Event::Status(s) if s == n.to_string())
            );
        }
        assert!(matches!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            Event::Ready
        ));
        assert!(worker.join().unwrap().is_ok());
        let (tx, rx) = mpsc::sync_channel(1);
        drop(rx);
        assert!(publish(&tx, Event::Ready).is_err());
    }

    use futures_util::{SinkExt, StreamExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::{accept_async, tungstenite::Message, WebSocketStream};

    async fn next_non_map_frame(
        socket: &mut WebSocketStream<TcpStream>,
    ) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
        loop {
            let frame = socket.next().await?;
            if let Ok(Message::Text(text)) = &frame {
                if let Ok(ClientMessage::Request {
                    request_id,
                    request,
                }) = serde_json::from_str::<ClientMessage>(text)
                {
                    if let Request::Map { query } = *request {
                        assert!(request_id.starts_with("map-"));
                        query.validate().unwrap();
                        assert!(query.cursor.is_none(), "unanswered maps cannot continue");
                        continue;
                    }
                }
            }
            return Some(frame);
        }
    }
    async fn receive(socket: &mut WebSocketStream<TcpStream>) -> ClientMessage {
        let frame = timeout(Duration::from_secs(2), next_non_map_frame(socket))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
    async fn send(socket: &mut WebSocketStream<TcpStream>, message: ServerMessage) {
        socket
            .send(Message::Text(
                serde_json::to_string(&message).unwrap().into(),
            ))
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_request_from_before_a_reset_is_refused_without_closing_the_connection() {
        queued_request_after_context_change(QueuedChange::Reset).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_request_from_before_an_authority_generation_change_is_refused() {
        queued_request_after_context_change(QueuedChange::Authority).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_advancement_query_from_an_older_observation_is_refused_without_disconnect() {
        queued_request_after_context_change(QueuedChange::Observation).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_run_step_from_an_older_observation_is_refused_without_retry() {
        queued_request_after_context_change(QueuedChange::MovementObservation).await;
    }

    #[derive(Clone, Copy)]
    enum QueuedChange {
        Reset,
        Authority,
        Observation,
        MovementObservation,
    }

    async fn queued_request_after_context_change(change: QueuedChange) {
        let reset = matches!(change, QueuedChange::Reset);
        let advancement = matches!(change, QueuedChange::Observation);
        let observation = matches!(
            change,
            QueuedChange::Observation | QueuedChange::MovementObservation
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let samples: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                format!("../protocol/tests/fixtures/wire-v{PROTOCOL_VERSION}.json"),
            ))
            .unwrap(),
        )
        .unwrap();
        let mut overview: AdvancementPage = serde_json::from_value(
            samples["server"]
                .as_array()
                .unwrap()
                .iter()
                .find(|sample| {
                    sample["type"] == "advancement_page"
                        && sample["page"]["query"]["kind"] == "overview"
                })
                .unwrap()["page"]
                .clone(),
        )
        .unwrap();
        let mut initial: Snapshot = serde_json::from_value(
            samples["server"]
                .as_array()
                .unwrap()
                .iter()
                .find(|sample| sample["type"] == "snapshot")
                .unwrap()["snapshot"]
                .clone(),
        )
        .unwrap();
        initial.has_control = true;
        let obsolete = initial.context.clone();
        let old_input = initial.readiness.revision;
        let old_revision = initial.state.revision;
        let branch = initial.branch.clone();
        let mut fresh = initial.clone();
        if reset {
            fresh.context.epoch += 1;
        } else if observation {
            std::sync::Arc::make_mut(&mut fresh.state).revision += 1;
        } else {
            fresh.readiness.revision += 1;
        }
        let current = fresh.context.clone();
        let current_generation = fresh.readiness.revision;
        let current_revision = fresh.state.revision;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            assert!(matches!(
                receive(&mut socket).await,
                ClientMessage::Hello { .. }
            ));
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![ActorId(1)],
                    role: AccessRole::Player,
                },
            )
            .await;
            assert!(matches!(
                receive(&mut socket).await,
                ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
            ));
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial),
                },
            )
            .await;
            let reply_branch = fresh.branch.clone();
            let mut reply_context = fresh.reply_context();
            if !reset {
                reply_context.cursor.sequence += 1;
            }
            send(
                &mut socket,
                if reset {
                    ServerMessage::Snapshot {
                        request_id: "reset".into(),
                        snapshot: Box::new(fresh),
                    }
                } else {
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: fresh.context,
                            actor: fresh.actor,
                            branch: fresh.branch,
                            cursor: StreamCursor {
                                sequence: fresh.cursor.sequence + 1,
                                tick: fresh.cursor.tick,
                            },
                            body: if observation {
                                UpdateBody::Observation {
                                    state: fresh.state,
                                    event: None,
                                }
                            } else {
                                UpdateBody::Readiness {
                                    readiness: fresh.readiness,
                                }
                            },
                        }),
                    }
                },
            )
            .await;
            if advancement {
                let ClientMessage::Request {
                    request_id,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("only an explicitly refreshed advancement query may reach the server")
                };
                let Request::Command {
                    context,
                    branch,
                    command:
                        tor_protocol::Command::Advancement {
                            expected_revision,
                            query: AdvancementQuery::Overview {},
                        },
                } = *request
                else {
                    panic!("only an explicitly refreshed advancement query may reach the server")
                };
                assert_eq!(context, reply_context.input);
                assert_eq!(branch, reply_context.branch);
                assert_eq!(expected_revision, reply_context.revision);
                overview.revision = reply_context.revision;
                overview.basis.actor = reply_context.actor;
                overview.basis.branch = reply_context.branch.clone();
                overview.basis.stream = reply_context.input.stream.stream.clone();
                overview.basis.reset = reply_context.input.stream.epoch;
                send(
                    &mut socket,
                    ServerMessage::AdvancementPage {
                        context: reply_context.clone(),
                        request_id,
                        page: Box::new(overview),
                    },
                )
                .await;
            }
            let ClientMessage::Request {
                request_id,
                request,
            } = receive(&mut socket).await
            else {
                panic!("stale gameplay request must not reach the server")
            };
            let Request::History { .. } = *request else {
                panic!("stale gameplay request must not reach the server")
            };
            send(
                &mut socket,
                ServerMessage::History {
                    context: reply_context.clone(),
                    request_id,
                    page: HistoryPage {
                        entries: vec![],
                        older_before: None,
                    },
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request,
            } = receive(&mut socket).await
            else {
                panic!("connection remains usable and closes with its save barrier")
            };
            let Request::Save = *request else {
                panic!("connection remains usable and closes with its save barrier")
            };
            send(
                &mut socket,
                ServerMessage::Ack {
                    context: reply_context,
                    request_id,
                    receipt: RequestReceipt::Immediate {
                        actor: ActorId(1),
                        branch: reply_branch,
                        entry_id: None,
                    },
                },
            )
            .await;
        });
        let (commands, rx) = async_mpsc::channel(4);
        let (events, received) = mpsc::sync_channel(64);
        let mut worker = tokio::spawn(async move {
            run(
                address,
                "token".into(),
                ActorId(1),
                true,
                Duration::ZERO,
                rx,
                &events,
            )
            .await
        });
        loop {
            let event = received.recv_timeout(Duration::from_secs(2)).unwrap();
            let changed = match event {
                Event::Snapshot(snapshot) => reset && snapshot.context == current,
                Event::Update(update) => match &update.body {
                    UpdateBody::Readiness { readiness } => {
                        !reset && !observation && readiness.revision == current_generation
                    }
                    UpdateBody::Observation { state, .. } => {
                        observation && state.revision == current_revision
                    }
                    _ => false,
                },
                _ => false,
            };
            if changed {
                break;
            }
        }
        for _ in 0..if advancement { 2 } else { 1 } {
            commands
                .send(Command::Request {
                    context: obsolete.clone(),
                    request: Box::new(Request::Command {
                        context: InputContext {
                            stream: obsolete.clone(),
                            readiness_revision: old_input,
                        },
                        branch: branch.clone(),
                        command: if advancement {
                            tor_protocol::Command::Advancement {
                                expected_revision: old_revision,
                                query: AdvancementQuery::Overview {},
                            }
                        } else {
                            tor_protocol::Command::Act {
                                expected_revision: old_revision,
                                action: Action::Move {
                                    direction: Direction::East,
                                },
                            }
                        },
                    }),
                })
                .await
                .unwrap();
            loop {
                let event = match received.recv_timeout(Duration::from_secs(2)) {
                    Ok(event) => event,
                    Err(error) => {
                        let outcome = timeout(Duration::from_secs(2), &mut worker).await;
                        panic!("expected a recoverable stale-query refusal; event channel: {error:?}; network result: {outcome:?}");
                    }
                };
                if match event {
                    Event::Fatal(error) => panic!("stale query became a fatal error: {error}"),
                    Event::Refused(status) if advancement => {
                        assert!(status.contains("Advancement"));
                        assert!(status.contains("not sent"));
                        true
                    }
                    Event::Refused(status) if !advancement => {
                        assert!(status.starts_with("State changed; request was not sent"));
                        // Match the native loop: a rejected queued step may be
                        // followed by Ready, but neither can restart the run.
                        let mut app = tor_client_ascii::App::new();
                        app.running = Some(Direction::East);
                        app.busy = true;
                        app.refused(status);
                        app.ready();
                        assert_eq!(app.running, None);
                        assert_eq!(app.continue_run(), tor_client_ascii::Effect::None);
                        true
                    }
                    Event::Status(status)
                        if !advancement
                            && status.starts_with("State changed; request was not sent") =>
                    {
                        panic!("a rejected queued run must be refused, not left running: {status}")
                    }
                    _ => false,
                } {
                    break;
                }
            }
        }
        if advancement {
            commands
                .send(Command::Request {
                    context: current.clone(),
                    request: Box::new(Request::Command {
                        context: InputContext {
                            stream: current.clone(),
                            readiness_revision: current_generation,
                        },
                        branch,
                        command: tor_protocol::Command::Advancement {
                            expected_revision: current_revision,
                            query: AdvancementQuery::Overview {},
                        },
                    }),
                })
                .await
                .unwrap();
            loop {
                if let Event::AdvancementPage { context, page, .. } =
                    received.recv_timeout(Duration::from_secs(2)).unwrap()
                {
                    assert_eq!(context.revision, current_revision);
                    assert_eq!(page.revision, current_revision);
                    break;
                }
            }
        }
        commands
            .send(Command::Request {
                context: current,
                request: Box::new(Request::History {
                    before: None,
                    limit: 1,
                }),
            })
            .await
            .unwrap();
        loop {
            if matches!(
                received.recv_timeout(Duration::from_secs(2)).unwrap(),
                Event::History(_)
            ) {
                break;
            }
        }
        drop(commands);
        worker.await.unwrap().unwrap();
        server.await.unwrap();
    }
    #[tokio::test]
    async fn a_confirmed_reply_during_recovery_is_retained_until_the_snapshot() {
        for rejected in [false, true] {
            let samples: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
                    format!("../protocol/tests/fixtures/wire-v{PROTOCOL_VERSION}.json"),
                ))
                .unwrap(),
            )
            .unwrap();
            let initial: Snapshot = serde_json::from_value(
                samples["server"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|sample| sample["type"] == "snapshot")
                    .unwrap()["snapshot"]
                    .clone(),
            )
            .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let (reply_sent, sent) = tokio::sync::oneshot::channel();
            let (reset_allowed, allowed) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut socket = accept_async(stream).await.unwrap();
                assert!(matches!(
                    receive(&mut socket).await,
                    ClientMessage::Hello { .. }
                ));
                send(
                    &mut socket,
                    ServerMessage::Welcome {
                        protocol: PROTOCOL_VERSION,
                        capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                        user: "test".into(),
                        actors: vec![initial.actor],
                        role: AccessRole::Player,
                    },
                )
                .await;
                assert!(matches!(
                    receive(&mut socket).await,
                    ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. })
                ));
                send(
                    &mut socket,
                    ServerMessage::Snapshot {
                        request_id: "attach".into(),
                        snapshot: Box::new(initial.clone()),
                    },
                )
                .await;
                let ClientMessage::Request {
                    request_id: pending,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("one original save required")
                };
                let Request::Save = *request else {
                    panic!("one original save required")
                };
                send(
                    &mut socket,
                    ServerMessage::Update {
                        update: Box::new(StreamUpdate {
                            context: initial.context.clone(),
                            actor: initial.actor,
                            branch: initial.branch.clone(),
                            cursor: StreamCursor {
                                sequence: initial.cursor.sequence + 2,
                                tick: initial.cursor.tick,
                            },
                            body: UpdateBody::Control { has_control: false },
                        }),
                    },
                )
                .await;
                let ClientMessage::Request {
                    request_id: reset,
                    request,
                } = receive(&mut socket).await
                else {
                    panic!("one recovery request required")
                };
                let Request::Snapshot = *request else {
                    panic!("one recovery request required")
                };
                let reply = if rejected {
                    ServerMessage::Error {
                        scope: ErrorScope::Attached {
                            context: initial.reply_context(),
                        },
                        request_id: Some(pending),
                        code: ErrorCode::StorageFailure,
                        message: "Save rejected".into(),
                    }
                } else {
                    ServerMessage::Ack {
                        context: initial.reply_context(),
                        request_id: pending,
                        receipt: RequestReceipt::Immediate {
                            actor: initial.actor,
                            branch: initial.branch.clone(),
                            entry_id: None,
                        },
                    }
                };
                send(&mut socket, reply).await;
                reply_sent.send(()).unwrap();
                allowed.await.unwrap();
                let mut fresh = initial;
                fresh.context.epoch += 1;
                fresh.cursor.sequence += 2;
                send(
                    &mut socket,
                    ServerMessage::Snapshot {
                        request_id: reset,
                        snapshot: Box::new(fresh),
                    },
                )
                .await;
                if let Some(Ok(Message::Text(text))) = next_non_map_frame(&mut socket).await {
                    panic!("request must not be replayed: {text}");
                }
            });
            let connection = Connection::connect(address, "token".into(), ActorId(1), "test")
                .await
                .unwrap();
            let (events, received) = mpsc::sync_channel(64);
            let mut worker = tokio::spawn(async move {
                let mut connection = connection;
                transact(&mut connection, &mut (None, false), Request::Save, &events)
                    .await
                    .unwrap();
                assert!(connection.is_synchronized());
            });
            sent.await.unwrap();
            assert!(
                timeout(Duration::from_millis(20), &mut worker)
                    .await
                    .is_err(),
                "the confirmed reply must wait for a valid stream reset"
            );
            reset_allowed.send(()).unwrap();
            worker.await.unwrap();
            server.await.unwrap();
            let statuses: Vec<_> = received
                .try_iter()
                .filter_map(|event| match event {
                    Event::Status(status) | Event::Refused(status) => Some(status),
                    _ => None,
                })
                .collect();
            assert!(
                !statuses
                    .iter()
                    .any(|status| status.contains("previous request may have completed")),
                "confirmed reply became uncertain: {statuses:?}"
            );
            assert!(
                statuses.iter().any(|status| if rejected {
                    status == "The game couldn't be saved: Save rejected"
                } else {
                    status == "Done."
                }),
                "original reply missing: {statuses:?}"
            );
        }
    }

    fn timeout_chart_snapshot() -> Snapshot {
        use chart_pages::*;
        let mut observation = tor_client_common::test_support::observation();
        observation.charts = vec![ChartSummary {
            kind: ChartKind::Monsters,
            mode: ChartMode::Live,
            status: ChartStatus::Available,
            acquired_at: Some(observation.tick),
            chart: Some("timeout-sample".into()),
            latest_attempt: None,
        }];
        let snapshot = Snapshot {
            context: StreamContext {
                stream: StreamId("chart-timeout".into()),
                epoch: 0,
            },
            actor: observation.actor,
            branch: BranchId("main".into()),
            cursor: StreamCursor {
                sequence: 0,
                tick: observation.tick,
            },
            state: std::sync::Arc::new(StateView {
                wizard_game: false,
                revision: 0,
                observation,
            }),
            readiness: Readiness {
                revision: 0,
                admission: true,
                resume: vec![],
                cancel: vec![],
            },
            intentions: vec![],
            travel: None,
            has_control: false,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        };
        tor_client_common::ClientState::from_snapshot(snapshot.clone()).unwrap();
        snapshot
    }

    #[test]
    fn chart_terminal_failure_reaches_ui_even_when_next_message_is_unrelated() {
        use tor_client_common::charts::{ChartInput, ChartPages};
        let initial = timeout_chart_snapshot();
        let state = tor_client_common::ClientState::from_snapshot(initial.clone()).unwrap();
        let mut worker = ChartPages::default();
        let Some(Request::Chart { query }) = worker
            .request(ChartInput::Select(chart_pages::ChartKind::Monsters), &state)
            .unwrap()
        else {
            panic!("chart query")
        };
        worker.bind(&query, "chart-timeout", &state).unwrap();
        let mut ui = worker.clone();
        assert!(worker.reject("chart-timeout"));
        let unrelated = ServerMessage::Ack {
            context: initial.reply_context(),
            request_id: "unrelated".into(),
            receipt: RequestReceipt::Immediate {
                actor: initial.actor,
                branch: initial.branch.clone(),
                entry_id: None,
            },
        };
        let (tx, rx) = mpsc::sync_channel(8);
        publish_chart_result(true, &worker, &unrelated, &tx).unwrap();
        let Event::ChartPresentation(update) = rx.try_recv().unwrap() else {
            panic!("local failure presentation")
        };
        ui.install(update, &state).unwrap();
        assert!(!ui.loading());
        assert!(ui.lines(&state).join("\n").contains("request failed"));
        publish_chart_result(false, &worker, &unrelated, &tx).unwrap();
        assert!(
            rx.try_recv().is_err(),
            "unchanged failure must not be republished"
        );
        assert_eq!(state.snapshot(), initial);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn silent_chart_continuation_timeout_notifies_ui_without_disconnecting_or_sending_actions(
    ) {
        use chart_pages::*;
        use tor_client_common::charts::ChartInput;
        let initial = timeout_chart_snapshot();
        let before = initial.clone();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (continued, continuation_seen) = tokio::sync::oneshot::channel();
        let (resume, resumed) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            assert!(matches!(
                receive(&mut socket).await,
                ClientMessage::Hello { .. }
            ));
            send(
                &mut socket,
                ServerMessage::Welcome {
                    protocol: PROTOCOL_VERSION,
                    capabilities: ServerCapabilities::new(MAX_RESPONSE_BYTES as u32, 16),
                    user: "test".into(),
                    actors: vec![initial.actor],
                    role: AccessRole::Spectator,
                },
            )
            .await;
            assert!(
                matches!(receive(&mut socket).await, ClientMessage::Request { request, .. } if matches!(*request, Request::Attach { .. }))
            );
            send(
                &mut socket,
                ServerMessage::Snapshot {
                    request_id: "attach".into(),
                    snapshot: Box::new(initial.clone()),
                },
            )
            .await;
            let ClientMessage::Request {
                request_id,
                request,
            } = receive(&mut socket).await
            else {
                panic!("chart request")
            };
            let Request::Chart { query } = *request else {
                panic!("chart is read only")
            };
            assert_eq!(query.cursor, None);
            send(
                &mut socket,
                ServerMessage::ChartPage {
                    context: initial.reply_context(),
                    request_id,
                    page: Box::new(ChartPage {
                        context: query.context.clone(),
                        branch: query.branch.clone(),
                        base: query.base,
                        chart: query.chart.clone(),
                        kind: ChartKind::Monsters,
                        mode: ChartMode::Live,
                        acquired_at: initial.cursor.tick,
                        total_components: 1,
                        total_records: 2,
                        start: 0,
                        next: Some(1),
                        records: vec![ChartRecord::Component(ChartComponent {
                            component: 0,
                            alignment: ChartAlignment::Aligned,
                        })],
                    }),
                },
            )
            .await;
            let ClientMessage::Request { request, .. } = receive(&mut socket).await else {
                panic!("chart continuation")
            };
            assert!(matches!(*request, Request::Chart { query } if query.cursor == Some(1)));
            continued.send(()).unwrap();
            // Silence until the UI has actually received its local timeout event.
            resumed.await.unwrap();
            send(
                &mut socket,
                ServerMessage::Ack {
                    context: initial.reply_context(),
                    request_id: "still-connected".into(),
                    receipt: RequestReceipt::Immediate {
                        actor: initial.actor,
                        branch: initial.branch,
                        entry_id: None,
                    },
                },
            )
            .await;
            if let Some(Ok(Message::Text(text))) = next_non_map_frame(&mut socket).await {
                panic!("timeout must not send an action, snapshot request, or replay: {text}");
            }
        });
        let mut connection = Connection::connect(address, "token".into(), before.actor, "test")
            .await
            .unwrap();
        let request = connection
            .charts
            .request(ChartInput::Select(ChartKind::Monsters), &connection.state)
            .unwrap()
            .unwrap();
        let Request::Chart {
            query: expected_query,
        } = &request
        else {
            panic!("chart query")
        };
        let expected_query = expected_query.clone();
        connection.request(request).await.unwrap();
        let (events, received) = mpsc::sync_channel(64);
        let mut shown = (None, false);
        assert!(
            matches!(next(&mut connection, &mut shown, &events).await.unwrap(), ServerMessage::ChartPage { page, .. } if page.next == Some(1))
        );
        assert!(connection.charts.loading());
        let worker = tokio::spawn(async move {
            let message = next_with_chart_timeout(
                &mut connection,
                &mut shown,
                &events,
                Duration::from_millis(50),
            )
            .await
            .unwrap();
            assert!(
                matches!(message, ServerMessage::Ack { request_id, .. } if request_id == "still-connected")
            );
            assert!(connection.is_synchronized());
            assert!(!connection.charts.loading());
            assert_eq!(connection.charts.selected(), None);
            assert_eq!(connection.state.snapshot(), before);
            connection.close().await.unwrap();
        });
        continuation_seen.await.unwrap();
        let cancelled = tokio::task::spawn_blocking(move || loop {
            match received.recv_timeout(Duration::from_secs(2)).unwrap() {
                Event::ChartCancelled(query) => break query,
                Event::Palette(_) => {}
                _ => panic!("unexpected chart continuation presentation"),
            }
        })
        .await
        .unwrap();
        assert_eq!(cancelled, expected_query);
        resume.send(()).unwrap();
        worker.await.unwrap();
        server.await.unwrap();
    }
}
