//! Whole turns through the engine over a scripted server: what is sent, and
//! the one passage that comes back.
use std::collections::VecDeque;
use std::time::Duration;

use tor_client_common::{ClientState, Palette};
use tor_client_text::engine::{turn::Link, Engine, Outcome};
use tor_protocol::*;

/// What a scripted server sends in answer to a request.
#[allow(clippy::large_enum_variant)] // Test frames are few and short-lived.
enum Frame {
    Ack,
    Reject(ErrorCode),
    /// The next observation, with the action event it follows.
    View(StateView, Option<Event>),
    /// The status of the journey this request started.
    Journey(TravelPhase),
}

type Responder = Box<dyn FnMut(&Request, &StateView) -> Vec<Frame>>;

/// A server that answers requests from a script, through the real client
/// state so every update is validated as the real connection's would be.
struct Scripted {
    client: ClientState,
    palette: Palette,
    sent: Vec<Request>,
    queue: VecDeque<ServerMessage>,
    respond: Responder,
    sequence: u64,
    requests: u64,
    journey: Option<(EntryId, String, u64)>,
}

fn branch() -> BranchId {
    BranchId("main".into())
}

fn entry(id: String, tick: u64, content: HistoryContent) -> HistoryEntry {
    HistoryEntry {
        id: EntryId(id),
        branch: branch(),
        actor: ActorId(1),
        tick,
        author: Author::Backend {
            component: "test".into(),
        },
        audience: Audience::Actor,
        content,
    }
}

impl Scripted {
    fn new(
        state: StateView,
        respond: impl FnMut(&Request, &StateView) -> Vec<Frame> + 'static,
    ) -> Self {
        let snapshot = Snapshot {
            travel: None,
            actor: ActorId(1),
            branch: branch(),
            cursor: StreamCursor {
                sequence: 0,
                tick: state.observation.tick,
            },
            state,
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        };
        Self {
            client: ClientState::from_snapshot(snapshot).unwrap(),
            palette: Palette::default(),
            sent: Vec::new(),
            queue: VecDeque::new(),
            respond: Box::new(respond),
            sequence: 0,
            requests: 0,
            journey: None,
        }
    }

    fn update(&mut self, tick: u64, body: UpdateBody) -> ServerMessage {
        self.sequence += 1;
        ServerMessage::Update {
            update: Box::new(StreamUpdate {
                actor: ActorId(1),
                branch: branch(),
                cursor: StreamCursor {
                    sequence: self.sequence,
                    tick,
                },
                body,
            }),
        }
    }

    /// The tick and revision the queued messages will have reached.
    fn last(&self) -> (u64, u64) {
        let mut at = (
            self.client.state().observation.tick,
            self.client.state().revision,
        );
        for message in &self.queue {
            if let ServerMessage::Update { update } = message {
                if let UpdateBody::Observation { state, .. } = &update.body {
                    at = (state.observation.tick, state.revision);
                }
            }
        }
        at
    }
}

impl Link for Scripted {
    fn client(&self) -> &ClientState {
        &self.client
    }
    fn palette(&self) -> &Palette {
        &self.palette
    }
    fn role(&self) -> AccessRole {
        AccessRole::Player
    }
    fn set_pace(&mut self, _: Duration) {}
    fn pace(&self) -> Duration {
        Duration::ZERO
    }
    async fn send(
        &mut self,
        request: Request,
    ) -> Result<String, tor_client_text::engine::turn::Error> {
        self.requests += 1;
        let id = format!("r{}", self.requests);
        let frames = (self.respond)(&request, self.client.state());
        let destination = match &request {
            Request::Command {
                command: Command::Travel { destination, .. },
                ..
            } => Some(destination.clone()),
            _ => None,
        };
        for frame in frames {
            let message = match frame {
                Frame::Ack => ServerMessage::Ack {
                    request_id: id.clone(),
                    entry_id: destination
                        .as_ref()
                        .map(|_| EntryId(format!("journey-{id}"))),
                },
                Frame::Reject(code) => ServerMessage::Error {
                    request_id: Some(id.clone()),
                    code,
                    message: String::new(),
                },
                Frame::View(mut state, event) => {
                    let (tick, revision) = self.last();
                    state.revision = revision + 1;
                    state.observation.tick = state.observation.tick.max(tick);
                    let tick = state.observation.tick;
                    let event = event.map(|event| {
                        Box::new(entry(
                            format!("{id}-{}", self.sequence),
                            tick,
                            HistoryContent::Action {
                                action: Action::Wait,
                                event,
                            },
                        ))
                    });
                    self.update(
                        tick,
                        UpdateBody::Observation {
                            state: Box::new(state),
                            event,
                        },
                    )
                }
                Frame::Journey(phase) => {
                    let destination = destination.clone().expect("a journey");
                    let journey_id = EntryId(format!("journey-{id}"));
                    let (tick, _) = self.last();
                    let first = self
                        .journey
                        .as_ref()
                        .is_none_or(|(old, ..)| *old != journey_id);
                    let steps = if first {
                        0
                    } else {
                        self.journey.as_ref().unwrap().2 + 1
                    };
                    self.journey = Some((journey_id.clone(), destination.clone(), steps));
                    let entry = first.then(|| {
                        Box::new(entry(
                            journey_id.0.clone(),
                            tick,
                            HistoryContent::Travel {
                                destination: destination.clone(),
                            },
                        ))
                    });
                    self.update(
                        tick,
                        UpdateBody::Travel {
                            status: TravelStatus {
                                id: journey_id,
                                destination,
                                completed_steps: steps,
                                phase,
                            },
                            entry,
                        },
                    )
                }
            };
            self.queue.push_back(message);
        }
        self.sent.push(request);
        Ok(id)
    }
    async fn next(
        &mut self,
        _: Duration,
    ) -> Result<Option<ServerMessage>, tor_client_text::engine::turn::Error> {
        let Some(message) = self.queue.pop_front() else {
            return Ok(None);
        };
        if let ServerMessage::Update { update } = &message {
            self.client
                .apply(*update.clone())
                .map_err(|e| format!("{e:?}"))?;
        }
        Ok(Some(message))
    }
}

fn state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision":0,"observation":{
        "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":(0..7).map(|x| serde_json::json!({
            "key":format!("cell-{x}"),"position":{"x":x,"y":0,"z":0},
            "wall":false,"material":"stone","place_hint":x==1 || x==6,
            "stairs_up":false,"stairs_down":false
        })).collect::<Vec<_>>(),
        "ground_items":[
            {"reachable":true,"item":{"quantity":1,"appearance":"item","identified":true,"id":1,"name":"copper token","description":"A small copper disc."},"position":{"x":0,"y":0,"z":0}},
            {"reachable":false,"item":{"quantity":1,"appearance":"item","identified":true,"id":2,"name":"stone tablet","description":"A weathered slab of stone."},"position":{"x":6,"y":0,"z":0}}
        ],"inventory":[],"visible_actors":[],
        "combat":{"hp":50,"max_hp":50,"preparation_remaining":null,"preparation_active":false,
            "recovery_remaining":0,"actors":[],"events":[],"objective":null,
            "victory":false,"dead":false,"terminal":false}}})).unwrap()
}

/// The view after walking `steps` cells east.
fn east(mut s: StateView, steps: i32) -> StateView {
    for c in &mut s.observation.visible_cells {
        c.position.x -= steps;
    }
    for g in &mut s.observation.ground_items {
        g.position.x -= steps;
        g.reachable = g.position == Position { x: 0, y: 0, z: 0 };
    }
    for a in &mut s.observation.visible_actors {
        a.position.x -= steps;
    }
    s.observation.tick += 10 * steps as u64;
    s
}

fn figure(id: u64, name: &str, x: i32) -> ActorView {
    ActorView {
        name: name.into(),
        description: String::new(),
        id: ActorId(id),
        position: Position { x, y: 0, z: 0 },
        asset: None,
    }
}

fn take(mut s: StateView, item: u64) -> StateView {
    let index = s
        .observation
        .ground_items
        .iter()
        .position(|g| g.item.id == item)
        .unwrap();
    let g = s.observation.ground_items.remove(index);
    s.observation.inventory.push(g.item);
    s
}

fn not_ready(mut s: StateView) -> StateView {
    s.observation.ready = false;
    s
}

async fn play(link: &mut Scripted, engine: &mut Engine, line: &str) -> String {
    match engine.play(link, line).await.unwrap() {
        Outcome::Passage(text) | Outcome::Quit(text) => text,
    }
}

fn is_travel(request: &Request) -> bool {
    matches!(
        request,
        Request::Command {
            command: Command::Travel { .. },
            ..
        }
    )
}

fn is_act(request: &Request, wanted: &Action) -> bool {
    matches!(request, Request::Command { command: Command::Act { action, .. }, .. } if action == wanted)
}

/// A server where journeys arrive and pickups succeed.
fn obliging(request: &Request, now: &StateView) -> Vec<Frame> {
    match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let mut frames = vec![Frame::Ack];
            for step in 1..=6 {
                frames.push(Frame::View(
                    east(now.clone(), step),
                    Some(Event::Moved {
                        direction: Direction::East,
                    }),
                ));
            }
            frames.push(Frame::Journey(TravelPhase::Arrived));
            frames
        }
        Request::Command {
            command:
                Command::Act {
                    action: Action::Take { item, .. },
                    ..
                },
            ..
        } => vec![
            Frame::View(
                take(now.clone(), *item),
                Some(Event::Taken {
                    item: *item,
                    result: *item,
                    quantity: 1,
                }),
            ),
            Frame::Ack,
        ],
        Request::Command {
            command:
                Command::Act {
                    action: Action::Drop { item, .. },
                    ..
                },
            ..
        } => {
            let mut after = now.clone();
            let index = after
                .observation
                .inventory
                .iter()
                .position(|i| i.id == *item)
                .unwrap();
            let thing = after.observation.inventory.remove(index);
            after.observation.ground_items.push(GroundItemView {
                reachable: true,
                item: thing,
                position: Position { x: 0, y: 0, z: 0 },
            });
            vec![
                Frame::View(
                    after,
                    Some(Event::Dropped {
                        item: *item,
                        result: *item,
                        quantity: 1,
                    }),
                ),
                Frame::Ack,
            ]
        }
        _ => vec![Frame::Ack],
    }
}

#[tokio::test]
async fn an_approach_and_a_pickup_are_one_sentence() {
    let mut link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "get tablet").await;
    assert_eq!(text, "You walk over to the stone tablet and pick it up.");
    assert!(is_travel(&link.sent[0]));
    assert!(is_act(
        &link.sent[1],
        &Action::Take {
            item: 2,
            quantity: None
        }
    ));
    // "It" is what was just taken.
    assert_eq!(
        play(&mut link, &mut engine, "x it").await,
        "A weathered slab of stone."
    );
}

#[tokio::test]
async fn a_pickup_waits_for_the_character_to_be_ready_after_the_journey() {
    // The corpse bug: arriving while still recovering dropped the pickup.
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => vec![
            Frame::Ack,
            Frame::View(
                not_ready(east(now.clone(), 6)),
                Some(Event::Moved {
                    direction: Direction::East,
                }),
            ),
            Frame::Journey(TravelPhase::Arrived),
            // Others act; then it's the character's turn again.
            Frame::View(east(now.clone(), 6), None),
        ],
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "take tablet").await;
    assert_eq!(text, "You walk over to the stone tablet and pick it up.");
    assert_eq!(link.sent.len(), 2);
    assert_eq!(link.client.state().observation.inventory.len(), 1);
}

#[tokio::test]
async fn a_figure_coming_into_view_stops_the_journey_and_the_purpose_is_told() {
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let mut there = east(now.clone(), 1);
            there
                .observation
                .visible_actors
                .push(figure(2, "ruin scout", 4));
            vec![
                Frame::Ack,
                Frame::View(
                    there,
                    Some(Event::Moved {
                        direction: Direction::East,
                    }),
                ),
                Frame::Journey(TravelPhase::Hazard),
            ]
        }
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "take tablet").await;
    assert_eq!(
        text,
        "You head toward the stone tablet, intent on picking it up. A ruin scout comes into view to the east, and you stop warily."
    );
    assert_eq!(link.sent.len(), 1, "no pickup after the journey stopped");
    // The narration's newcomer is "it".
    let text = play(&mut link, &mut engine, "examine it").await;
    assert_eq!(text, "You see nothing special about the ruin scout.");
}

#[tokio::test]
async fn arriving_as_a_figure_appears_stops_short_of_the_pickup() {
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let mut there = east(now.clone(), 6);
            there.observation.visible_actors.push(figure(2, "rat", 1));
            vec![
                Frame::Ack,
                Frame::View(there, None),
                Frame::Journey(TravelPhase::Arrived),
            ]
        }
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "take the tablet").await;
    assert_eq!(
        text,
        "You walk over to the stone tablet, but stop short of picking it up as a rat comes into view to the east."
    );
    assert_eq!(link.sent.len(), 1);
}

fn combat(mut s: StateView, hp: u32, events: Vec<CombatEventView>) -> StateView {
    let c = s.observation.combat.as_mut().unwrap();
    c.hp = hp;
    c.events = events;
    s
}

#[tokio::test]
async fn an_exchange_of_blows_is_told_in_order_and_a_death_ends_it() {
    let mut start = state();
    start
        .observation
        .visible_actors
        .push(figure(2, "ruin scout", 1));
    let blows = std::cell::Cell::new(0);
    let mut link = Scripted::new(start, move |request, now| {
        let Request::Command {
            command:
                Command::Act {
                    action: Action::Attack { .. },
                    ..
                },
            ..
        } = request
        else {
            return vec![Frame::Ack];
        };
        blows.set(blows.get() + 1);
        if blows.get() == 1 {
            vec![
                Frame::View(
                    not_ready(now.clone()),
                    Some(Event::AttackStarted { target: ActorId(2) }),
                ),
                Frame::Ack,
                Frame::View(
                    combat(
                        now.clone(),
                        46,
                        vec![
                            CombatEventView::Attack {
                                attacker: Some(ActorId(2)),
                                target: Some(ActorId(1)),
                                outcome: AttackOutcome::Hit,
                            },
                            CombatEventView::Interrupted { actor: ActorId(1) },
                        ],
                    ),
                    None,
                ),
            ]
        } else {
            let mut after = combat(
                now.clone(),
                46,
                vec![
                    CombatEventView::Attack {
                        attacker: Some(ActorId(1)),
                        target: Some(ActorId(2)),
                        outcome: AttackOutcome::Hit,
                    },
                    CombatEventView::Died { actor: ActorId(2) },
                ],
            );
            after.observation.visible_actors.clear();
            vec![
                Frame::Ack,
                Frame::View(after, Some(Event::AttackStarted { target: ActorId(2) })),
            ]
        }
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "attack scout").await,
        "You ready an attack on the ruin scout. It strikes you, spoiling your attack.\nHP 46/50"
    );
    assert_eq!(
        play(&mut link, &mut engine, "kill it").await,
        "You strike the ruin scout, and it falls dead."
    );
    assert_eq!(
        play(&mut link, &mut engine, "attack it").await,
        "You can't see it any more."
    );
}

#[tokio::test]
async fn a_question_pauses_the_chain_and_its_answer_resumes_it() {
    let mut start = state();
    let mut silver = start.observation.ground_items[0].clone();
    silver.item.id = 3;
    silver.item.name = "silver token".into();
    silver.item.description = "A polished silver disc.".into();
    start.observation.ground_items.push(silver);
    let mut link = Scripted::new(start, obliging);
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "take token. examine it").await,
        "Which do you mean, the copper token or the silver token?"
    );
    assert!(link.sent.is_empty(), "asking takes no time");
    assert_eq!(
        play(&mut link, &mut engine, "nonsense").await,
        "Please choose the copper token or the silver token, or type a new command."
    );
    assert_eq!(
        play(&mut link, &mut engine, "the silver one").await,
        "You pick up the silver token.\nA polished silver disc."
    );
    // A new command abandons a question.
    assert_eq!(
        play(&mut link, &mut engine, "drop token. take token").await,
        "You drop the silver token.\nWhich do you mean, the copper token or the silver token?"
    );
    assert_eq!(
        play(&mut link, &mut engine, "inventory").await,
        "You are empty-handed."
    );
    assert_eq!(
        play(&mut link, &mut engine, "2").await,
        "There's no question to answer."
    );
}

#[tokio::test]
async fn a_refusal_stops_the_chain_and_says_why() {
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command:
                Command::Act {
                    action: Action::Take { .. },
                    ..
                },
            ..
        } => vec![Frame::Reject(ErrorCode::InvalidAction)],
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "take token, then take tablet").await,
        "You can't pick that up from here."
    );
    assert_eq!(link.sent.len(), 1);
}

#[tokio::test]
async fn simple_pickups_in_a_row_are_one_sentence_and_again_repeats() {
    let mut start = state();
    let mut twin = start.observation.ground_items[0].clone();
    twin.item.id = 3;
    start.observation.ground_items.push(twin);
    let mut link = Scripted::new(start, obliging);
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "take tokens").await,
        "You pick up the two copper tokens."
    );
    assert_eq!(
        play(&mut link, &mut engine, "again").await,
        "You already have the copper token."
    );
    assert_eq!(
        play(&mut link, &mut engine, "i").await,
        "You are carrying two copper tokens."
    );
}

#[tokio::test]
async fn what_happens_between_turns_is_one_passage() {
    let link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    engine.learn(&link);
    let before = link.client.state().clone();
    let mut link = link;
    let mut after = before.clone();
    after
        .observation
        .visible_actors
        .push(figure(5, "stone guardian", 3));
    let message = link.update(
        0,
        UpdateBody::Observation {
            state: Box::new({
                let mut a = after.clone();
                a.revision = 1;
                a
            }),
            event: None,
        },
    );
    if let ServerMessage::Update { update } = &message {
        link.client.apply(*update.clone()).unwrap();
    }
    let beats = engine.between_turns(&link, &before, &message);
    assert_eq!(
        engine.passage(&link, beats),
        "You notice a stone guardian to the east."
    );
    assert_eq!(
        play(&mut link, &mut engine, "x it").await,
        "You see nothing special about the stone guardian."
    );
}

#[tokio::test]
async fn verbs_the_game_cant_carry_out_yet_are_refused_plainly() {
    let mut link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    for (line, answer) in [
        ("wear the token", "You can't wear anything yet."),
        ("eat lamp", "You can't see any lamp here."),
        ("talk to me", "You can't talk with anyone yet."),
        ("jump", "You can't jump yet."),
        ("push", "What do you want to push?"),
        (
            "listen",
            "You listen closely. Aside from the faint whisper of air across the stone, all is quiet.",
        ),
        (
            "frobnicate the token",
            "I don't understand \"frobnicate the token\".",
        ),
    ] {
        assert_eq!(play(&mut link, &mut engine, line).await, answer, "{line}");
    }
    assert!(link.sent.is_empty());
}
