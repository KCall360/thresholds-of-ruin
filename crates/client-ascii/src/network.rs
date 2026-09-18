use std::{
    net::SocketAddr,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::Duration,
};
use tokio::{sync::mpsc as async_mpsc, time::timeout};
use tor_client_common::{ClientState, Connection};
use tor_protocol::*;

type Error = Box<dyn std::error::Error + Send + Sync>;
pub enum Event {
    Role(AccessRole),
    State(Box<ClientState>),
    Status(String),
    Ready,
    History(HistoryPage),
    Fatal(String),
}

pub struct Network {
    pub commands: async_mpsc::Sender<Request>,
    pub events: Receiver<Event>,
    worker: JoinHandle<()>,
}

impl Network {
    pub fn start(address: SocketAddr, token: String, actor: ActorId, observe: bool) -> Self {
        let (commands, rx) = async_mpsc::channel(1);
        let (tx, events) = mpsc::sync_channel(64);
        let worker = std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| -> Error { e.into() })
                .and_then(|runtime| runtime.block_on(run(address, token, actor, observe, rx, &tx)));
            if let Err(error) = result {
                let _=tx.try_send(Event::Fatal(format!("Connection ended: {error}. Relaunch to reconnect; inspect history before retrying.")));
            }
        });
        Self {
            commands,
            events,
            worker,
        }
    }

    pub fn shutdown(self) {
        drop(self.commands);
        // Drop the event receiver too, so no background publication can stall exit.
        drop(self.events);
        let _ = self.worker.join();
    }
}

fn publish(tx: &SyncSender<Event>, event: Event) -> Result<(), Error> {
    tx.try_send(event)
        .map_err(|_| "Window stopped consuming network updates".into())
}

async fn run(
    address: SocketAddr,
    token: String,
    actor: ActorId,
    observe: bool,
    mut rx: async_mpsc::Receiver<Request>,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let mut connection = Connection::connect(address, token, actor, "ascii").await?;
    publish(tx, Event::Role(connection.role()))?;
    publish(tx, Event::State(Box::new(connection.state.clone())))?;
    if connection.role() == AccessRole::Spectator {
        publish(
            tx,
            Event::Status("Spectator access is read-only. F2: history; Esc: close.".into()),
        )?;
    } else if !observe {
        transact(&mut connection, Request::AcquireControl, tx).await?;
    } else {
        publish(
            tx,
            Event::Status("Observing. Press C to request control.".into()),
        )?;
    }
    publish(tx, Event::Ready)?;
    loop {
        tokio::select! {
            message=connection.next()=>{present(&connection,message?,tx)?;},
            request=rx.recv()=>{
                let Some(request)=request else {connection.close().await;return Ok(());};
                transact(&mut connection,request,tx).await?;
                publish(tx,Event::Ready)?;
            },
        }
    }
}

async fn transact(
    connection: &mut Connection,
    request: Request,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    let id = connection.request(request).await?;
    timeout(Duration::from_secs(10), async {
        loop {
            let message = connection.next().await?;
            let complete = match &message {
                ServerMessage::Ack { request_id, .. }
                | ServerMessage::History { request_id, .. }
                | ServerMessage::Snapshot { request_id, .. } => request_id == &id,
                ServerMessage::Error { request_id, .. } => request_id.as_ref() == Some(&id),
                _ => false,
            };
            present(connection, message, tx)?;
            if complete {
                return Ok::<(), Error>(());
            }
        }
    })
    .await
    .map_err(|_| "Response timed out; the last request may have committed")??;
    Ok(())
}

fn present(
    connection: &Connection,
    message: ServerMessage,
    tx: &SyncSender<Event>,
) -> Result<(), Error> {
    match message {
        ServerMessage::Update { .. } | ServerMessage::Snapshot { .. } => {
            publish(tx, Event::State(Box::new(connection.state.clone())))
        }
        ServerMessage::Ack { .. } => publish(
            tx,
            Event::Status("Ready. Your accepted actions and notes are saved automatically.".into()),
        ),
        ServerMessage::Error { code, message, .. } => {
            publish(tx, Event::Status(format!("{code:?}: {message}")))
        }
        ServerMessage::History { page, .. } => {
            publish(tx, Event::History(page))?;
            publish(tx, Event::Status("History loaded.".into()))
        }
        ServerMessage::Welcome { .. } => Err("Unexpected repeated welcome".into()),
    }
}
