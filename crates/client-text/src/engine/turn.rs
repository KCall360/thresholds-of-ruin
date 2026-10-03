//! Running a turn: each goal's steps against the server, until the game is
//! waiting for this player again.
use std::time::Duration;

use tor_client_common::{ClientState, Connection, Palette};
use tor_protocol::*;

use super::{
    chronicle::{Beat, Chronicler},
    narrate::{End, Entry, Episode, Record},
    prose,
    scene::{direction_name, Key, Kind, Scene},
    verbs::Goal,
};

pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// How long to wait for an answer before giving up on the server.
const ANSWER: Duration = Duration::from_secs(10);
/// How long play may stay quiet, with the character still not ready, before
/// the turn ends anyway: another player's character, or one nothing
/// controls, may be next.
const QUIET: Duration = Duration::from_secs(2);

/// The connection a turn runs over. Tests use scripted links.
#[allow(async_fn_in_trait)]
pub trait Link {
    fn client(&self) -> &ClientState;
    fn palette(&self) -> &Palette;
    fn role(&self) -> AccessRole;
    fn set_pace(&mut self, pace: Duration);
    fn pace(&self) -> Duration;
    /// Send a request; returns its id.
    async fn send(&mut self, request: Request) -> Result<String, Error>;
    /// The next server message, already applied to the client state; `None`
    /// if none arrives within `wait`.
    async fn next(&mut self, wait: Duration) -> Result<Option<ServerMessage>, Error>;
}

impl Link for Connection {
    fn client(&self) -> &ClientState {
        &self.state
    }
    fn palette(&self) -> &Palette {
        &self.palette
    }
    fn role(&self) -> AccessRole {
        Connection::role(self)
    }
    fn set_pace(&mut self, pace: Duration) {
        Connection::set_pace(self, pace);
    }
    fn pace(&self) -> Duration {
        Connection::pace(self)
    }
    async fn send(&mut self, request: Request) -> Result<String, Error> {
        self.request(request).await
    }
    async fn next(&mut self, wait: Duration) -> Result<Option<ServerMessage>, Error> {
        match tokio::time::timeout(wait, Connection::next(self)).await {
            Ok(message) => message.map(Some),
            Err(_) => Ok(None),
        }
    }
}

/// What a request waits for before it counts as finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Until {
    /// An action: acknowledged, its result seen, and the character ready.
    Acted { revision: u64 },
    /// A journey: acknowledged, ended, and the character ready.
    Journey,
    /// Resumed play: the character ready.
    Ready,
    /// A reply: the acknowledgement, snapshot or history page.
    Answered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settled {
    Done,
    Rejected(ErrorCode),
    /// A journey ended this way.
    Journey(TravelPhase),
    /// Control was lost, or the state replaced.
    Lost,
}

/// Reads updates for a turn, recording beats as they come.
pub struct Reader<'a> {
    pub chronicler: &'a mut Chronicler,
    pub beats: Vec<Beat>,
    /// History pages answered during the turn, as text.
    pub pages: Vec<String>,
    pub resynced: bool,
}

impl Reader<'_> {
    /// Apply one message and record its beats.
    pub fn record(&mut self, before: &StateView, link: &impl Link, message: &ServerMessage) {
        let after = link.client().state();
        match message {
            ServerMessage::Update { update } => match &update.body {
                UpdateBody::Observation { event, .. }
                | UpdateBody::ObservationDelta { event, .. } => {
                    let beats =
                        self.chronicler
                            .observe(before, after, event.as_deref(), link.palette());
                    self.beats.extend(beats);
                }
                UpdateBody::Travel { status, .. } if status.phase != TravelPhase::Active => {
                    self.beats.push(Beat::Journey {
                        phase: status.phase,
                    });
                }
                UpdateBody::Control { has_control } => {
                    self.beats.push(Beat::Control(*has_control));
                }
                UpdateBody::Travel { .. } | UpdateBody::Annotation { .. } => {}
            },
            ServerMessage::Snapshot { .. } => {
                self.resynced = true;
                self.beats.push(Beat::Resync);
            }
            ServerMessage::History { page, .. } => {
                if page.entries.is_empty() {
                    self.pages.push("There is no history to show.".into());
                }
                for entry in &page.entries {
                    self.pages.push(crate::history(entry));
                }
                if let Some(before) = &page.older_before {
                    self.pages
                        .push(format!("Older entries: history {}", crate::safe(&before.0)));
                }
            }
            _ => {}
        }
    }
}

fn terminal(state: &StateView) -> bool {
    state
        .observation
        .combat
        .as_ref()
        .is_some_and(|c| c.terminal || c.dead)
}

/// Send `request` and read until it's finished as `until` says.
async fn settle(
    link: &mut impl Link,
    reader: &mut Reader<'_>,
    request: Request,
    until: Until,
) -> Result<Settled, Error> {
    let id = link.send(request).await?;
    let mut acked = false;
    let mut receipt: Option<EntryId> = None;
    let mut ended = None;
    loop {
        let before = link.client().state().clone();
        let wait = if acked { QUIET } else { ANSWER };
        let Some(message) = link.next(wait).await? else {
            if acked {
                // Play went quiet without this player being next.
                return Ok(match ended {
                    Some(phase) => Settled::Journey(phase),
                    None if until == Until::Journey => Settled::Journey(TravelPhase::Active),
                    None => Settled::Done,
                });
            }
            return Err("The server did not answer. The action may have completed; reconnect and check history before trying again.".into());
        };
        reader.record(&before, link, &message);
        match &message {
            ServerMessage::Error {
                request_id, code, ..
            } if request_id.as_ref() == Some(&id) => return Ok(Settled::Rejected(*code)),
            ServerMessage::Ack {
                request_id,
                entry_id,
            } if *request_id == id => {
                acked = true;
                receipt = entry_id.clone();
            }
            ServerMessage::Snapshot { request_id, .. } if *request_id == id => {
                return Ok(Settled::Done);
            }
            ServerMessage::History { request_id, .. } if *request_id == id => {
                return Ok(Settled::Done);
            }
            ServerMessage::Snapshot { .. } => return Ok(Settled::Lost),
            ServerMessage::Update { update } => {
                if let UpdateBody::Control { has_control: false } = update.body {
                    if until != Until::Answered {
                        return Ok(Settled::Lost);
                    }
                }
            }
            _ => {}
        }
        if !acked {
            continue;
        }
        let state = link.client().state();
        let ready = state.observation.ready || terminal(state);
        match until {
            Until::Answered => return Ok(Settled::Done),
            Until::Ready if ready => return Ok(Settled::Done),
            Until::Acted { revision } if ready && state.revision > revision => {
                return Ok(Settled::Done)
            }
            Until::Journey => {
                ended = link
                    .client()
                    .travel()
                    .filter(|t| Some(&t.id) == receipt.as_ref())
                    .map(|t| t.phase)
                    .filter(|phase| *phase != TravelPhase::Active);
                if let Some(phase) = ended {
                    if ready {
                        return Ok(Settled::Journey(phase));
                    }
                }
                if receipt.is_none() {
                    return Ok(Settled::Journey(TravelPhase::Failed));
                }
            }
            _ => {}
        }
    }
}

/// One step of a goal, decided from the scene in hand.
enum Step {
    Act(Action),
    Travel(String),
    Finish(End),
}

fn step(goal: &Goal, scene: &Scene, acted: bool, approached: bool) -> Step {
    if acted {
        return Step::Finish(End::Done);
    }
    let act = Step::Act;
    let travel = |key: Key| match scene.approach(key) {
        Some(cell) => Step::Travel(cell),
        None => Step::Finish(End::Refused("You can't see a way to get there.".into())),
    };
    match goal {
        Goal::Take { item, quantity } => match scene.get(Key::Item(*item)) {
            Some(r) if r.reachable => act(Action::Take {
                item: *item,
                quantity: *quantity,
            }),
            Some(_) if !approached => travel(Key::Item(*item)),
            _ => Step::Finish(End::Gone),
        },
        Goal::Drop { item, quantity } => act(Action::Drop {
            item: *item,
            quantity: *quantity,
        }),
        Goal::Door { door, open } => match scene.get(Key::Door(*door)) {
            Some(r) if r.open == Some(*open) => Step::Finish(End::Done),
            Some(r) if r.reachable => act(Action::SetDoor {
                door: *door,
                open: *open,
            }),
            Some(_) if !approached => travel(Key::Door(*door)),
            _ => Step::Finish(End::Gone),
        },
        Goal::Attack { target } => match scene.get(Key::Actor(*target)) {
            Some(r) if r.reachable || approached => act(Action::Attack { target: *target }),
            Some(_) => match scene.approach(Key::Actor(*target)) {
                Some(cell) => Step::Travel(cell),
                None => act(Action::Attack { target: *target }),
            },
            None => Step::Finish(End::Gone),
        },
        Goal::Approach { target } if !approached => travel(*target),
        Goal::Go { destination, .. } if !approached => Step::Travel(destination.clone()),
        Goal::Approach { .. } | Goal::Go { .. } => Step::Finish(End::Done),
        Goal::Step { direction } => act(Action::Move {
            direction: *direction,
        }),
        Goal::Wait => act(Action::Wait),
    }
}

/// How the goal's object is named as it begins.
fn object(goal: &Goal, scene: &Scene) -> String {
    let named = |key: Key| scene.get(key).map(|r| r.the());
    match goal {
        Goal::Take { item, quantity } | Goal::Drop { item, quantity } => {
            match (scene.get(Key::Item(*item)), quantity) {
                (Some(r), Some(q)) if *q < r.quantity => Some(prose::counted(*q, &r.name)),
                (Some(r), _) => Some(r.the()),
                (None, _) => None,
            }
        }
        Goal::Door { door, .. } => named(Key::Door(*door)),
        Goal::Attack { target } => named(Key::Actor(*target)),
        Goal::Approach { target } => named(*target),
        Goal::Go { direction, .. } | Goal::Step { direction } => {
            Some(direction_name(*direction).to_owned())
        }
        Goal::Wait => None,
    }
    .unwrap_or_else(|| "it".into())
}

fn refusal(code: ErrorCode, goal: &Goal) -> String {
    match code {
        ErrorCode::InvalidAction => match goal {
            Goal::Take { .. } => "You can't pick that up from here.",
            Goal::Drop { .. } => "You can't drop that here.",
            Goal::Door { .. } => "You can't reach it from here.",
            Goal::Attack { .. } => "You can't reach it from here.",
            Goal::Step { .. } => "You can't go that way.",
            Goal::Go { .. } | Goal::Approach { .. } => "You can't find a way there.",
            Goal::Wait => "You can't wait right now.",
        },
        ErrorCode::NotController | ErrorCode::ControlTaken => {
            "Another player has control. You are only watching."
        }
        ErrorCode::ActorBusy => "You are still on your way.",
        ErrorCode::Unauthorized => "You aren't allowed to do that.",
        ErrorCode::StaleRevision | ErrorCode::WrongBranch => {
            "Things have changed. Look around and try again."
        }
        ErrorCode::StorageFailure => "The game couldn't save that, so it didn't happen.",
        _ => "That couldn't be done.",
    }
    .into()
}

/// Whether this connection may act, or why not.
pub fn may_act(link: &impl Link) -> Result<(), &'static str> {
    if link.role() == AccessRole::Spectator {
        Err("Spectator access is read-only.")
    } else if !link.client().has_control() {
        Err("You are observing. Use control to take over when it is available.")
    } else if terminal(link.client().state()) {
        Err("This run has ended.")
    } else {
        Ok(())
    }
}

/// Run one goal to its end, adding its episode to the record. Returns
/// whether it was done, so the chain goes on.
pub async fn run_goal(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    goal: Goal,
) -> Result<bool, Error> {
    let start = Scene::new(link.client().state(), link.palette());
    let object = object(&goal, &start);
    let known = start.figure_ids();
    drop(start);
    let mut episode = Episode {
        goal: goal.clone(),
        object,
        approached: false,
        beats: Vec::new(),
        end: End::Done,
    };
    let mut reader = Reader {
        chronicler,
        beats: Vec::new(),
        pages: Vec::new(),
        resynced: false,
    };
    // After reconnecting during recovery, play resumes before anything new.
    let state = link.client().state();
    if !state.observation.ready
        && link
            .client()
            .travel()
            .is_none_or(|t| t.phase != TravelPhase::Active)
    {
        if settle(link, &mut reader, Request::Continue, Until::Ready).await? == Settled::Lost {
            episode.end = End::Lost;
        } else if !link.client().state().observation.ready {
            episode.end = End::Refused("It isn't your turn to act yet.".into());
        }
    }
    let mut acted = false;
    while episode.end == End::Done {
        let scene = Scene::new(link.client().state(), link.palette());
        let next = step(&goal, &scene, acted, episode.approached);
        let revision = scene.state.revision;
        let branch = link.client().branch().clone();
        drop(scene);
        match next {
            Step::Finish(end) => {
                episode.end = end;
                break;
            }
            Step::Act(action) => {
                let request = Request::Command {
                    branch,
                    command: Command::Act {
                        expected_revision: revision,
                        action,
                    },
                };
                match settle(link, &mut reader, request, Until::Acted { revision }).await? {
                    Settled::Done | Settled::Journey(_) => acted = true,
                    Settled::Rejected(code) => episode.end = End::Refused(refusal(code, &goal)),
                    Settled::Lost => episode.end = End::Lost,
                }
            }
            Step::Travel(destination) => {
                let request = Request::Command {
                    branch,
                    command: Command::Travel {
                        expected_revision: revision,
                        destination,
                    },
                };
                episode.approached = true;
                match settle(link, &mut reader, request, Until::Journey).await? {
                    Settled::Journey(TravelPhase::Arrived) => {
                        // Arrival never authorizes what comes next if someone
                        // new came into view.
                        let now = Scene::new(link.client().state(), link.palette());
                        let newcomer = now.figure_ids().difference(&known).next().is_some();
                        let target_is_figure = matches!(goal, Goal::Attack { .. })
                            || matches!(goal, Goal::Approach { target } if now.get(target).is_some_and(|r| r.is(Kind::Figure)));
                        if newcomer
                            && !matches!(goal, Goal::Go { .. } | Goal::Approach { .. })
                            && !target_is_figure
                        {
                            episode.end = End::Wary;
                        }
                    }
                    Settled::Journey(phase) => {
                        if phase == TravelPhase::Blocked {
                            // Name who's in the way, when someone is.
                            let now = Scene::new(link.client().state(), link.palette());
                            let blocker = now.of(Kind::Figure).find(|r| r.reachable);
                            if let Some(r) = blocker {
                                if let Key::Actor(id) = r.key {
                                    reader.beats.push(Beat::Barred(super::chronicle::Figure {
                                        id,
                                        name: r.name.clone(),
                                    }));
                                }
                            }
                        }
                        episode.end = End::Stopped(phase);
                    }
                    Settled::Done => episode.end = End::Stopped(TravelPhase::Active),
                    Settled::Rejected(code) => {
                        episode.approached = false;
                        episode.end = End::Refused(refusal(code, &goal));
                    }
                    Settled::Lost => episode.end = End::Lost,
                }
            }
        }
    }
    let done = episode.end == End::Done;
    episode.beats = std::mem::take(&mut reader.beats);
    let resynced = reader.resynced;
    record.entries.push(Entry::Episode(episode));
    Ok(done && !resynced)
}

/// Send a session request and record what comes back.
pub async fn run_request(
    link: &mut impl Link,
    chronicler: &mut Chronicler,
    record: &mut Record,
    request: Request,
) -> Result<bool, Error> {
    if !link.role().permits(&request) {
        record.say("Spectator access is read-only.");
        return Ok(false);
    }
    let mut reader = Reader {
        chronicler,
        beats: Vec::new(),
        pages: Vec::new(),
        resynced: false,
    };
    let settled = settle(link, &mut reader, request, Until::Answered).await?;
    let beats = std::mem::take(&mut reader.beats);
    let pages = std::mem::take(&mut reader.pages);
    if !beats.is_empty() {
        record.entries.push(Entry::Beats(beats));
    }
    if !pages.is_empty() {
        record.say(pages.join("\n"));
    }
    if let Settled::Rejected(code) = settled {
        record.say(match code {
            ErrorCode::NotController | ErrorCode::ControlTaken => {
                "Another player has control. You are only watching."
            }
            ErrorCode::Unauthorized => "You aren't allowed to do that.",
            ErrorCode::StaleRevision | ErrorCode::WrongBranch => {
                "Things have changed. Look around and try again."
            }
            ErrorCode::StorageFailure => "The game couldn't save that.",
            ErrorCode::InvalidAnchor | ErrorCode::InvalidAnnotation => {
                "That note can't be attached there."
            }
            _ => "That couldn't be done.",
        });
        return Ok(false);
    }
    Ok(settled == Settled::Done)
}
