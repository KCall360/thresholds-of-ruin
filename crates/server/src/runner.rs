//! The session's single owner: a thread that runs the game until it needs a
//! client's input, handling its mailbox between actions. No timer advances
//! the game; see `docs/run-until-blocked.md`.

use crate::session::{Account, Connection, Service, Step, HEADROOM};
use crate::Failure;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, oneshot};
use tor_protocol::Request;

pub(crate) enum Mail {
    Connect {
        account: Account,
        frontend: String,
        reply: oneshot::Sender<Result<Connection, Failure>>,
    },
    Request {
        client: u64,
        request_id: String,
        request: Request,
        /// When the socket read it, if timing diagnostics are on.
        timing: Option<crate::diagnostics::RequestTiming>,
    },
    Disconnect(u64),
    /// Run something on the service between actions.
    Call(Box<dyn FnOnce(&mut Service) + Send>),
    Shutdown(oneshot::Sender<Option<crate::storage::Store>>),
}

/// How often save progress is checked: acknowledgements and the "behind
/// schedule" warning. It never advances the game.
const SAVE_POLL: Duration = Duration::from_millis(50);

/// A client whose queue stays nearly full this long is disconnected, like a
/// socket write that times out.
pub(crate) const STALL: Duration = Duration::from_secs(5);

/// The running session: a thread that owns the [`Service`] and runs the game
/// until it needs a client's input. [`crate::serve`] connects clients to it.
pub struct Simulation {
    pub(crate) diagnostics: Option<crate::diagnostics::Diagnostics>,
    pub(crate) mail: mpsc::Sender<Mail>,
    pub(crate) thread: std::thread::JoinHandle<Service>,
}

impl Simulation {
    /// Start the simulation thread. It stops when [`crate::serve`] shuts it
    /// down, or when every handle to it is dropped.
    pub fn start(mut service: Service) -> Self {
        let diagnostics = crate::diagnostics::Diagnostics::stderr();
        service.set_diagnostics(diagnostics.clone());
        let (mail, mailbox) = mpsc::channel(256);
        let thread = std::thread::Builder::new()
            .name("tor-simulation".into())
            .spawn(move || {
                tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                    .expect("simulation runtime")
                    .block_on(run(service, mailbox, STALL))
            })
            .expect("simulation thread");
        Self {
            mail,
            thread,
            diagnostics,
        }
    }

    pub fn handle(&self) -> SimulationHandle {
        SimulationHandle(self.mail.clone())
    }
}

/// Reaches into a running [`Simulation`], for tests and tools.
#[derive(Clone)]
pub struct SimulationHandle(mpsc::Sender<Mail>);

impl SimulationHandle {
    /// Run `call` on the service between actions; `None` once it has stopped.
    pub async fn with<R: Send + 'static>(
        &self,
        call: impl FnOnce(&mut Service) -> R + Send + 'static,
    ) -> Option<R> {
        let (reply, answer) = oneshot::channel();
        let call = Box::new(move |service: &mut Service| {
            let _ = reply.send(call(service));
        });
        self.0.send(Mail::Call(call)).await.ok()?;
        answer.await.ok()
    }
}

/// Handle one message; false once shut down.
fn receive(service: &mut Service, mail: Mail) -> bool {
    match mail {
        Mail::Connect {
            account,
            frontend,
            reply,
        } => {
            let _ = reply.send(service.connect(&account, frontend));
        }
        Mail::Request {
            client,
            request_id,
            request,
            timing,
        } => {
            let Some(timing) = timing else {
                service.handle(client, request_id, request);
                return true;
            };
            let handling = Instant::now();
            service.handle(client, request_id.clone(), request);
            timing.diagnostics.timing(
                "server_handled",
                client,
                &request_id,
                (handling - timing.started).as_secs_f64() * 1000.,
                handling.elapsed().as_secs_f64() * 1000.,
            );
        }
        Mail::Disconnect(client) => service.disconnect(client),
        Mail::Call(call) => call(service),
        Mail::Shutdown(reply) => {
            service.shutdown();
            let _ = reply.send(service.flush_handle());
            return false;
        }
    }
    true
}

/// Run the session until shut down. A client whose queue stays nearly full
/// for `stall` is disconnected.
pub(crate) async fn run(
    mut service: Service,
    mut mailbox: mpsc::Receiver<Mail>,
    stall: Duration,
) -> Service {
    let mut saves_polled = Instant::now();
    // When each nearly-full client was first seen nearly full.
    let mut stalled: BTreeMap<u64, Instant> = BTreeMap::new();
    loop {
        // Handle every message that could already be queued, but do not let
        // continuously arriving replacements starve saves or the simulation.
        for _ in 0..mailbox.max_capacity() {
            match mailbox.try_recv() {
                Ok(mail) => {
                    if !receive(&mut service, mail) {
                        return service;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    service.shutdown();
                    return service;
                }
            }
        }
        // Reaching the batch limit must not add an action after the final
        // sender disappeared and its last queued message was handled.
        if mailbox.is_closed() && mailbox.is_empty() {
            service.shutdown();
            return service;
        }
        if saves_polled.elapsed() >= SAVE_POLL {
            service.poll_saves();
            saves_polled = Instant::now();
        }
        let full = match service.step() {
            Step::Progress => {
                stalled.clear();
                continue;
            }
            Step::Blocked => {
                stalled.clear();
                Vec::new()
            }
            Step::Full(full) => full,
        };
        let now = Instant::now();
        stalled.retain(|id, _| full.iter().any(|(client, _)| client == id));
        for (client, _) in &full {
            stalled.entry(*client).or_insert(now);
        }
        let expired: Vec<_> = stalled
            .iter()
            .filter(|(_, since)| now - **since >= stall)
            .map(|(&client, _)| client)
            .collect();
        if !expired.is_empty() {
            for client in expired {
                stalled.remove(&client);
                service.disconnect(client);
            }
            continue;
        }
        let poll = tokio::time::sleep_until((saves_polled + SAVE_POLL).into());
        let stall_deadline = stalled.values().min().map(|since| *since + stall);
        let space = async {
            match full.first() {
                Some((_, sender)) => sender.wait_for_headroom(HEADROOM).await,
                None => std::future::pending().await,
            }
        };
        let deadline = async {
            match stall_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline.into()).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            mail = mailbox.recv() => {
                let Some(mail) = mail else {
                    service.shutdown();
                    return service;
                };
                if !receive(&mut service, mail) {
                    return service;
                }
            }
            _ = space => {}
            _ = deadline => {}
            _ = poll => {}
        }
    }
}
