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
    /// Play stopped; whose move it is.
    Waiting(Waiting),
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
                Frame::Waiting(on) => ServerMessage::Waiting { on },
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
    let (told, arrival) = text.split_once('\n').unwrap();
    assert_eq!(told, "You walk over to the stone tablet and pick it up.");
    // The tablet was in another place, so that place is described: the
    // character was walked into it.
    assert!(arrival.starts_with("You are in an open"), "{arrival}");
    assert!(
        arrival.contains("A copper token lies to the west."),
        "{arrival}"
    );
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
    assert_eq!(
        text.lines().next(),
        Some("You walk over to the stone tablet and pick it up.")
    );
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
    // The sighting is why the pickup waits, so it's told even though the
    // new place's description names the rat too.
    assert_eq!(
        text.lines().next(),
        Some("You walk over to the stone tablet, but stop short of picking it up as a rat comes into view to the east.")
    );
    assert!(text.contains("There is a rat to the east."), "{text}");
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
            "frobnicate the token",
            "I don't understand \"frobnicate the token\".",
        ),
    ] {
        assert_eq!(play(&mut link, &mut engine, line).await, answer, "{line}");
    }
    // Listening is answered by the place's atmosphere, and takes no time.
    let heard = play(&mut link, &mut engine, "listen").await;
    assert!(
        !heard.is_empty() && !heard.starts_with("You can't"),
        "{heard}"
    );
    assert!(link.sent.is_empty());
}

#[tokio::test]
async fn stacks_of_alike_things_are_counted_together() {
    // Regression: three stacks of arrows were "the ten arrows, the five
    // arrows, the two arrows", and two stacks of three potions "the two
    // three red potionses".
    let mut s = state();
    let stack = |id: u64, name: &str, quantity: u64| {
        serde_json::from_value::<GroundItemView>(serde_json::json!({
            "reachable": true, "position": {"x": 0, "y": 0, "z": 0},
            "item": {"quantity": quantity, "appearance": "item", "identified": true,
                "id": id, "name": name, "description": ""}}))
        .unwrap()
    };
    s.observation.ground_items = vec![
        stack(10, "arrow", 10),
        stack(11, "arrow", 5),
        stack(12, "arrow", 2),
        stack(13, "red potion", 3),
        stack(14, "red potion", 3),
    ];
    let mut link = Scripted::new(s, obliging);
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "take all").await,
        "You pick up the 17 arrows and the six red potions."
    );
}

#[tokio::test]
async fn a_place_is_described_in_full_once_and_then_named_unless_verbose() {
    // Journeys go east to the second hinted place and back west.
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let there = now.observation.visible_cells[0].position.x != 0;
            let (view, direction) = if there {
                let mut back = now.clone();
                for c in &mut back.observation.visible_cells {
                    c.position.x += 6;
                }
                for g in &mut back.observation.ground_items {
                    g.position.x += 6;
                    g.reachable = g.position == Position { x: 0, y: 0, z: 0 };
                }
                back.observation.tick += 60;
                (back, Direction::West)
            } else {
                (east(now.clone(), 6), Direction::East)
            };
            vec![
                Frame::Ack,
                Frame::View(view, Some(Event::Moved { direction })),
                Frame::Journey(TravelPhase::Arrived),
            ]
        }
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    assert!(engine.welcome(&link).contains("You are in an open"));
    let there = play(&mut link, &mut engine, "east").await;
    assert!(
        there.starts_with("You walk east.\nYou are in an open"),
        "{there}"
    );
    // Back where the game began: already described, so only named.
    let back = play(&mut link, &mut engine, "west").await;
    assert!(
        back.starts_with("You walk west.\nYou are back in the open"),
        "{back}"
    );
    assert!(back.contains("A copper token lies at your feet"), "{back}");
    assert_eq!(
        play(&mut link, &mut engine, "verbose").await,
        "Places are described in full every time you arrive."
    );
    let again = play(&mut link, &mut engine, "east").await;
    assert!(
        again.starts_with("You walk east.\nYou are in an open"),
        "{again}"
    );
    play(&mut link, &mut engine, "superbrief").await;
    let named = play(&mut link, &mut engine, "west").await;
    assert!(
        named.starts_with("You walk west.\nYou are back in"),
        "{named}"
    );
    assert!(!named.contains("You can head"), "{named}");
    // Looking always describes in full.
    assert!(play(&mut link, &mut engine, "look")
        .await
        .contains("You are in an open"));
}

#[tokio::test]
async fn a_remembered_place_is_travelled_to_by_name() {
    let mut s = state();
    s.observation.places = vec![PlaceView {
        key: "cell-6".into(),
        name: "Far Hall".into(),
        origin: PlaceNameOrigin::Authored,
    }];
    let mut link = Scripted::new(s, obliging);
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "go to far hall").await;
    assert!(
        text.starts_with(
            "You make your way back to Far Hall.
Far Hall
You are in "
        ),
        "{text}"
    );
    assert!(matches!(
        &link.sent[0],
        Request::Command { command: Command::Travel { destination, .. }, .. } if destination == "cell-6"
    ));
}

#[tokio::test]
async fn walking_across_open_ground_stays_in_one_place() {
    // No walls or hints: all of it is one open place, so arriving isn't a
    // new place to describe, and a creature seen on the way is still told.
    let mut s = state();
    for cell in &mut s.observation.visible_cells {
        cell.place_hint = false;
    }
    let mut link = Scripted::new(s, |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let mut there = east(now.clone(), 6);
            there.observation.visible_actors.push(figure(2, "rat", 3));
            vec![
                Frame::Ack,
                Frame::View(
                    there,
                    Some(Event::Moved {
                        direction: Direction::East,
                    }),
                ),
                Frame::Journey(TravelPhase::Arrived),
            ]
        }
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "east").await;
    assert!(text.starts_with("You walk east."), "{text}");
    assert!(text.contains("rat"), "{text}");
    assert!(!text.contains("You are"), "{text}");
}

#[tokio::test]
async fn a_figure_that_steps_up_to_meet_an_attack_is_attacked() {
    // Regression: closing in on a figure that stepped into the way ended the
    // turn with "the way is blocked", though it was right there.
    let mut start = state();
    start
        .observation
        .visible_actors
        .push(figure(2, "ruin scout", 4));
    let mut link = Scripted::new(start, |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let mut met = now.clone();
            met.observation.visible_actors[0].position.x = 1;
            vec![
                Frame::Ack,
                Frame::View(met, None),
                Frame::Journey(TravelPhase::Blocked),
            ]
        }
        Request::Command {
            command:
                Command::Act {
                    action: Action::Attack { .. },
                    ..
                },
            ..
        } => {
            let mut after = combat(
                now.clone(),
                50,
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
        _ => vec![Frame::Ack],
    });
    let mut engine = Engine::default();
    let text = play(&mut link, &mut engine, "attack scout").await;
    assert!(!text.contains("blocked"), "{text}");
    assert!(
        text.starts_with("You close in on the ruin scout."),
        "{text}"
    );
    assert!(text.contains("falls dead"), "{text}");
    assert_eq!(link.sent.len(), 2);
}

#[tokio::test]
async fn a_figure_that_backs_away_keeps_out_of_reach() {
    // Regression: "You walk over to the ember wisp. You can't reach it from
    // here." when it had moved off while the character closed in.
    let mut start = state();
    start
        .observation
        .visible_actors
        .push(figure(2, "ember wisp", 4));
    let mut link = Scripted::new(start, |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            let mut there = east(now.clone(), 2);
            there.observation.visible_actors[0].position.x = 3;
            vec![
                Frame::Ack,
                Frame::View(
                    there,
                    Some(Event::Moved {
                        direction: Direction::East,
                    }),
                ),
                Frame::Journey(TravelPhase::Arrived),
            ]
        }
        Request::Command {
            command: Command::Act { .. },
            ..
        } => vec![Frame::Reject(ErrorCode::InvalidAction)],
        _ => vec![Frame::Ack],
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "attack wisp").await,
        "You go after the ember wisp. It keeps out of reach."
    );
}

#[tokio::test]
async fn a_creature_in_the_way_is_named() {
    // Regression: the creature barring a journey was found after the journey
    // ended but looked for before, so it was never named.
    let mut start = state();
    start
        .observation
        .visible_actors
        .push(figure(2, "ruin guard", 1));
    let mut link = Scripted::new(start, |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => vec![Frame::Ack, Frame::Journey(TravelPhase::Blocked)],
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "take tablet").await,
        "You head toward the stone tablet, intent on picking it up, but the ruin guard bars the way."
    );
}

/// A server for walks across open ground: each journey goes six cells east
/// and reveals six more cells beyond, and on the `find`th journey a pebble
/// comes into view ahead.
fn open_ground(find: usize) -> Scripted {
    let mut start = state();
    for cell in &mut start.observation.visible_cells {
        cell.place_hint = false;
    }
    start.observation.ground_items.clear();
    let legs = std::cell::Cell::new(0usize);
    Scripted::new(start, move |request, now| match request {
        Request::Command {
            command: Command::Travel { .. },
            ..
        } => {
            legs.set(legs.get() + 1);
            let mut there = east(now.clone(), 6);
            let mut far = there.observation.visible_cells[0].clone();
            for x in 1..=6 {
                far.key = format!("far-{}-{x}", legs.get());
                far.position.x = x;
                there.observation.visible_cells.push(far.clone());
            }
            if legs.get() == find {
                there.observation.ground_items.push(
                    serde_json::from_value(serde_json::json!({
                        "reachable": false, "position": {"x": 5, "y": 0, "z": 0},
                        "item": {"quantity": 1, "appearance": "item", "identified": true,
                            "id": 7, "name": "pebble", "description": ""}}))
                    .unwrap(),
                );
            }
            vec![
                Frame::Ack,
                Frame::View(
                    there,
                    Some(Event::Moved {
                        direction: Direction::East,
                    }),
                ),
                Frame::Journey(TravelPhase::Arrived),
            ]
        }
        other => obliging(other, now),
    })
}

#[tokio::test]
async fn a_walk_into_darkness_goes_on_until_something_comes_into_view() {
    let mut link = open_ground(3);
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "east").await,
        "You walk east until you see a pebble to the east."
    );
    assert_eq!(link.sent.len(), 3);
    assert!(link.sent.iter().all(is_travel));
}

#[tokio::test]
async fn a_walk_with_nothing_to_find_stops_in_the_end() {
    let mut link = open_ground(usize::MAX);
    let mut engine = Engine::default();
    assert_eq!(play(&mut link, &mut engine, "east").await, "You walk east.");
    assert_eq!(link.sent.len(), 12);
}

#[tokio::test]
async fn a_turn_ends_when_the_server_says_another_player_is_next() {
    // The server's word ends the turn at once; what comes after it is
    // another player's doing, told between turns.
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command: Command::Act { .. },
            ..
        } => {
            let mut later = not_ready(now.clone());
            later.observation.visible_actors.push(figure(2, "rat", 3));
            vec![
                Frame::View(not_ready(now.clone()), Some(Event::Waited)),
                Frame::Ack,
                Frame::Waiting(Waiting::Others),
                Frame::View(later, None),
            ]
        }
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    assert_eq!(play(&mut link, &mut engine, "wait").await, "Time passes.");
    assert_eq!(
        link.queue.len(),
        1,
        "the rat's arrival waits for between turns"
    );
}

#[tokio::test]
async fn a_doorway_out_of_sight_a_moment_later_is_still_a_way_out() {
    // Regression: beside the wall, only the head-height cell of a doorway
    // was in sight, so the room had no way out until the character moved.
    let room = ["#####", "#.@.'..", "#####"];
    let start = crate::adventure::walled(&room);
    let mut hidden = start.clone();
    // The doorway's floor drops out of sight.
    hidden
        .observation
        .visible_cells
        .retain(|c| !(c.position.x == 2 && c.position.y == 0 && c.position.z == 0));
    let mut link = Scripted::new(start, move |request, _| match request {
        Request::Command {
            command: Command::Act { .. },
            ..
        } => vec![Frame::View(hidden.clone(), Some(Event::Waited)), Frame::Ack],
        _ => vec![Frame::Ack],
    });
    let mut engine = Engine::default();
    engine.welcome(&link);
    play(&mut link, &mut engine, "wait").await;
    let look = play(&mut link, &mut engine, "look").await;
    assert!(look.contains("An open oak door leads east."), "{look}");
}
