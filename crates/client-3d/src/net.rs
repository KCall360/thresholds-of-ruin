//! Bounded transport. One worker owns shared state and admits one UI command.
use std::{
    fmt,
    net::SocketAddr,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tokio::{sync::mpsc as async_mpsc, time::timeout};
use tor_client_3d_model::{request_from, view};
pub use tor_client_3d_model::{Cmd, Happening, Input, View};
use tor_client_common::{Connection, PendingRequest, RequestCompletion};
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;
const EVENT_CAPACITY: usize = 64;
const EVENTS_PER_FRAME: usize = 16;

pub enum Event {
    View(Box<View>),
    Status(String),
    Ready,
    Fatal(String),
}

pub struct Network {
    commands: async_mpsc::Sender<Input>,
    events: Receiver<Event>,
    ready: bool,
    closed: bool,
    fatal_reported: bool,
    failure: Option<String>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl Network {
    fn channels() -> (Self, async_mpsc::Receiver<Input>, SyncSender<Event>) {
        let (commands, rx) = async_mpsc::channel(1);
        let (tx, events) = mpsc::sync_channel(EVENT_CAPACITY);
        (
            Self {
                commands,
                events,
                ready: false,
                closed: false,
                fatal_reported: false,
                failure: None,
                worker: None,
            },
            rx,
            tx,
        )
    }

    pub fn try_command(&mut self, command: Input) -> Result<(), &'static str> {
        if self.closed {
            return Err("Connection closed");
        }
        if !self.ready {
            return Err("Waiting for the current request");
        }
        self.ready = false;
        self.commands
            .try_send(command)
            .map_err(|_| "Connection is not accepting input")
    }

    fn next_event(&mut self) -> Result<Event, mpsc::TryRecvError> {
        let event = self.events.try_recv()?;
        match &event {
            Event::Ready => self.ready = true,
            Event::Fatal(_) => {
                self.ready = false;
                self.fatal_reported = true;
            }
            _ => {}
        }
        Ok(event)
    }

    /// Preserve ordering, limit work, and let deferred scene entities materialize
    /// before another view can refer to them.
    pub fn frame_events(&mut self) -> Vec<Event> {
        let started = Instant::now();
        let mut batch = Vec::new();
        for _ in 0..EVENTS_PER_FRAME {
            if started.elapsed() >= Duration::from_millis(4) {
                break;
            }
            match self.next_event() {
                Ok(event) => {
                    let view = matches!(event, Event::View(_));
                    batch.push(event);
                    if view {
                        break;
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) if !self.closed && !self.fatal_reported => {
                    self.ready = false;
                    self.fatal_reported = true;
                    batch.push(Event::Fatal("Network worker stopped unexpectedly".into()));
                    break;
                }
                Err(_) => break,
            }
        }
        batch
    }

    pub fn start(address: SocketAddr, token: String, actor: ActorId, observe: bool) -> Self {
        let (mut network, rx, tx) = Self::channels();
        network.worker = Some(std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(Error::from)
                .and_then(|rt| rt.block_on(run(address, token, actor, observe, rx, &tx)))
                .map_err(|error| error.to_string());
            if let Err(error) = &result {
                let _ = tx.send(Event::Fatal(format!("Connection ended: {error}")));
            }
            result
        }));
        network
    }

    /// Drop the window's receiver before joining: a worker waiting for render
    /// backpressure must wake up and execute the shared durable close barrier.
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.closed = true;
        self.ready = false;
        let (closed, _) = async_mpsc::channel(1);
        drop(std::mem::replace(&mut self.commands, closed));
        let (_, disconnected) = mpsc::channel();
        drop(std::mem::replace(&mut self.events, disconnected));
        if let Some(worker) = self.worker.take() {
            self.failure = match worker.join() {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some("Network worker panicked; save completion is unconfirmed".into()),
            };
        }
        self.failure.clone().map_or(Ok(()), Err)
    }
}

impl Drop for Network {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[derive(Debug)]
struct WindowClosed;
impl fmt::Display for WindowClosed {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("Window closed")
    }
}
impl std::error::Error for WindowClosed {}

fn publish(tx: &SyncSender<Event>, event: Event) -> Result<(), Error> {
    tx.send(event).map_err(|_| Box::new(WindowClosed) as Error)
}

fn present(
    connection: &Connection,
    message: &ServerMessage,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    match message {
        ServerMessage::Update { .. }
        | ServerMessage::Snapshot { .. }
        | ServerMessage::Palette { .. }
        | ServerMessage::MapPage { .. } => publish(
            tx,
            Event::View(Box::new(view(&connection.state, &connection.palette))),
        ),
        ServerMessage::Error { message, .. } => publish(tx, Event::Status(message.clone())),
        _ => Ok(()),
    }
}

async fn run(
    address: SocketAddr,
    token: String,
    actor: ActorId,
    observe: bool,
    rx: async_mpsc::Receiver<Input>,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let mut connection = Connection::connect(address, token, actor, "3d").await?;
    connection.use_external_advancement_presentation();
    connection.set_pace(Duration::from_millis(90));
    let result = session(&mut connection, observe, rx, tx).await;
    // A broken presentation pipe is normal during window shutdown. Connection
    // failures remain errors, and a failed save always takes precedence.
    connection.close().await?;
    match result {
        Err(error) if !error.is::<WindowClosed>() => Err(error),
        _ => Ok(()),
    }
}

async fn session(
    connection: &mut Connection,
    observe: bool,
    mut rx: async_mpsc::Receiver<Input>,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    publish(
        tx,
        Event::View(Box::new(view(&connection.state, &connection.palette))),
    )?;
    if !observe && connection.role() != AccessRole::Spectator {
        transact(connection, Request::AcquireControl, tx).await?;
    }
    publish(tx, Event::Ready)?;
    loop {
        tokio::select! {
            biased;
            command = rx.recv() => match command {
                None => return Ok(()),
                Some(command) => {
                    match request_from(&connection.state, &command.shown, command.command) {
                        Ok(request) if connection.is_synchronized() => transact(connection, request, tx).await?,
                        Ok(_) => publish(tx, Event::Status("Resynchronizing; try again.".into()))?,
                        Err(reason) => publish(tx, Event::Status(reason))?,
                    }
                    publish(tx, Event::Ready)?;
                }
            },
            message = connection.next() => {
                let message = message?;
                present(connection, &message, tx)?;
            }
        }
    }
}

async fn transact(
    connection: &mut Connection,
    request: Request,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let id = connection.request(request).await?;
    let mut pending = PendingRequest::new(id);
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            let completion = pending.observe(connection, &message);
            let complete = completion.is_some();
            let unknown = matches!(completion, Some(RequestCompletion::Unknown));
            present(connection, &message, tx)?;
            if unknown {
                publish(
                    tx,
                    Event::Status("Resynchronized; the last request may have completed.".into()),
                )?;
            }
            if complete {
                return Ok::<(), Error>(());
            }
        }
    })
    .await
    .map_err(|_| "Response timed out")??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_input_keeps_its_presented_context_after_the_view_advances() {
        let (mut network, mut commands, events) = Network::channels();
        events.send(Event::Ready).unwrap();
        network.frame_events();
        let net = crate::Net(std::sync::Arc::new(std::sync::Mutex::new(network)));
        let mut game = crate::Game {
            view: Some(crate::test_support::view(7)),
            ..Default::default()
        };
        let shown = game.view.as_ref().unwrap().shown.clone();
        assert!(game.send(&net, Cmd::Control));
        game.view = Some(crate::test_support::view(8));
        let input = commands.try_recv().unwrap();
        assert_eq!(input.shown, shown);
        assert_ne!(input.shown, game.view.as_ref().unwrap().shown);
    }

    #[test]
    fn a_closed_inventory_cannot_dispatch_its_old_pressed_button() {
        use bevy::prelude::{App, BackgroundColor, Button, Interaction, Update};
        for open in [true, false] {
            let (mut network, mut commands, events) = Network::channels();
            events.send(Event::Ready).unwrap();
            network.next_event().unwrap();
            let net = crate::Net(std::sync::Arc::new(std::sync::Mutex::new(network)));
            let target = ItemTarget::from_digest([9; 32]);
            let mut view = crate::test_support::view(0);
            view.has_control = true;
            view.can_act = true;
            view.inventory.push(tor_client_3d_model::Carried {
                model: tor_client_3d_model::ItemModel::Placeholder,
                id: target,
                name: "item".into(),
                class: ItemClass::Misc,
                quantity: 1,
                description: String::new(),
                equipped: None,
                can_equip: false,
                drinkable: false,
                edible: false,
            });
            let game = crate::Game {
                view: Some(view),
                ..Default::default()
            };
            let mut panel = crate::interact::Panel::default();
            panel.open = open;
            let mut app = App::new();
            app.insert_resource(net)
                .insert_resource(game)
                .insert_resource(panel)
                .add_systems(Update, crate::interact::panel_buttons);
            app.world_mut().spawn((
                Button,
                Interaction::Pressed,
                crate::interact::PanelButton::Drop(target),
                BackgroundColor::default(),
            ));
            app.update();
            assert_eq!(
                commands.try_recv().is_ok(),
                open,
                "hidden inventory dispatched an action"
            );
        }
    }

    #[test]
    fn render_backpressure_is_bounded_and_status_order_is_preserved() {
        let (mut network, _, events) = Network::channels();
        for index in 0..EVENT_CAPACITY {
            assert!(events.try_send(Event::Status(index.to_string())).is_ok());
        }
        assert!(matches!(
            events.try_send(Event::Ready),
            Err(mpsc::TrySendError::Full(_))
        ));
        let mut observed = Vec::new();
        for _ in 0..EVENT_CAPACITY * 4 {
            let batch = network.frame_events();
            assert!(batch.len() <= EVENTS_PER_FRAME);
            for event in batch {
                if let Event::Status(status) = event {
                    observed.push(status);
                } else {
                    panic!("reordered status");
                }
            }
            if observed.len() == EVENT_CAPACITY {
                break;
            }
        }
        assert_eq!(
            observed,
            (0..EVENT_CAPACITY)
                .map(|index| index.to_string())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shutdown_releases_a_worker_blocked_by_a_full_render_queue() {
        let (mut network, _, events) = Network::channels();
        let (filled, observed) = mpsc::channel();
        network.worker = Some(std::thread::spawn(move || {
            for _ in 0..EVENT_CAPACITY {
                events.send(Event::Ready).unwrap();
            }
            filled.send(()).unwrap();
            assert!(
                events.send(Event::Ready).is_err(),
                "closing the window must release blocked publishing"
            );
            Ok(())
        }));
        observed.recv_timeout(Duration::from_secs(5)).unwrap();
        network.shutdown().unwrap();
    }

    #[test]
    fn save_failure_survives_repeated_shutdown_calls() {
        let (mut network, _, _) = Network::channels();
        network.worker = Some(std::thread::spawn(|| Err("save failed".into())));
        assert_eq!(network.shutdown(), Err("save failed".into()));
        assert_eq!(network.shutdown(), Err("save failed".into()));
    }

    #[test]
    fn a_view_is_applied_before_the_next_view_or_its_ready_marker() {
        let sample = crate::test_support::view;
        let (mut network, _, events) = Network::channels();
        events.send(Event::View(Box::new(sample(1)))).unwrap();
        events.send(Event::View(Box::new(sample(2)))).unwrap();
        events.send(Event::Ready).unwrap();
        for revision in [1, 2] {
            let batch = network.frame_events();
            assert!(matches!(batch.as_slice(), [Event::View(view)] if view.revision == revision));
            assert!(!network.ready);
        }
        assert!(matches!(network.frame_events().as_slice(), [Event::Ready]));
        assert!(network.ready);
    }

    #[test]
    fn commands_wait_for_initial_readiness_and_do_not_queue_behind_an_action() {
        let (mut network, mut commands, events) = Network::channels();
        assert!(
            network
                .try_command(crate::test_support::input(Cmd::Wait))
                .is_err(),
            "startup is not ready for input"
        );
        events.send(Event::Ready).unwrap();
        assert!(matches!(network.next_event(), Ok(Event::Ready)));
        assert!(network
            .try_command(crate::test_support::input(Cmd::Wait))
            .is_ok());
        assert!(commands.try_recv().is_ok());
        assert!(
            network
                .try_command(crate::test_support::input(Cmd::Wait))
                .is_err(),
            "a running action must not accumulate stale input"
        );
        events.send(Event::Ready).unwrap();
        network.next_event().unwrap();
        assert!(network
            .try_command(crate::test_support::input(Cmd::Wait))
            .is_ok());
    }

    #[test]
    fn repeated_input_cannot_queue_while_the_worker_is_running_an_action() {
        let (mut network, mut commands, events) = Network::channels();
        events.send(Event::Ready).unwrap();
        network.next_event().unwrap();
        assert!(network
            .try_command(crate::test_support::input(Cmd::Wait))
            .is_ok());
        commands.try_recv().unwrap();
        for _ in 0..100 {
            assert!(
                network
                    .try_command(crate::test_support::input(Cmd::Wait))
                    .is_err(),
                "held input queued another action"
            );
        }
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn a_panicked_worker_cannot_look_like_successful_save_and_quit() {
        let (mut network, _, _) = Network::channels();
        network.worker = Some(std::thread::spawn(|| panic!("injected worker failure")));
        assert!(
            network.shutdown().is_err(),
            "the process must report its worker failure"
        );
    }
}
