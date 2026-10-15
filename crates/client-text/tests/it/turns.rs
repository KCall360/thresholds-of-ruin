//! Whole turns through the engine over a scripted server: what is sent, and
//! the one passage that comes back.
use std::collections::VecDeque;
use std::time::Duration;
use tor_client_common::test_support::equipment_slot;

use tor_client_common::{ClientState, Palette};
use tor_client_text::engine::{turn::Link, Engine, Outcome};
use tor_protocol::*;

/// What a scripted server sends in answer to a request.
#[allow(clippy::large_enum_variant)] // Test frames are few and short-lived.
enum Frame {
    Ack,
    Admitted,
    Readiness(Readiness),
    Intention(IntentionPhase),
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
            readiness: tor_protocol::Readiness {
                revision: 0,
                admission: true,
                resume: vec![],
                cancel: vec![],
            },
            context: fixture_context(),
            intentions: Vec::new(),
            travel: None,
            actor: ActorId(1),
            branch: branch(),
            cursor: StreamCursor {
                sequence: 0,
                tick: state.observation.tick,
            },
            state: state.into(),
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
                context: fixture_context(),
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

    fn reply_context(&self) -> ReplyContext {
        let mut projected = self.client.clone();
        for message in &self.queue {
            if let ServerMessage::Update { update } = message {
                projected.apply(*update.clone()).unwrap();
            }
        }
        projected.reply_context()
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
    fn is_synchronized(&self) -> bool {
        true
    }
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
                Frame::Readiness(readiness) => {
                    let (tick, _) = self.last();
                    self.update(tick, UpdateBody::Readiness { readiness })
                }
                Frame::Admitted => ServerMessage::Ack {
                    context: self.reply_context(),
                    request_id: id.clone(),
                    receipt: RequestReceipt::Admitted {
                        actor: ActorId(1),
                        branch: branch(),
                        intention: IntentionId(format!("intent-{id}")),
                        entry_id: EntryId(format!("intent-{id}")),
                        phase: IntentionPhase::Queued,
                    },
                },
                Frame::Intention(phase) => {
                    let (tick, _) = self.last();
                    self.update(
                        tick,
                        UpdateBody::Intention {
                            status: IntentionStatus {
                                actor: ActorId(1),
                                branch: branch(),
                                intention: IntentionId(format!("intent-{id}")),
                                entry_id: EntryId(format!("intent-{id}")),
                                phase,
                            },
                        },
                    )
                }
                Frame::Ack => ServerMessage::Ack {
                    context: self.reply_context(),
                    request_id: id.clone(),
                    receipt: RequestReceipt::Immediate {
                        actor: ActorId(1),
                        branch: branch(),
                        entry_id: destination
                            .as_ref()
                            .map(|_| EntryId(format!("journey-{id}"))),
                    },
                },
                Frame::Reject(code) => ServerMessage::Error {
                    scope: ErrorScope::Attached {
                        context: self.reply_context(),
                    },
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
                            state: state.into(),
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
        "wizard_game":false,"revision":"0","observation":{
        "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":0,"y":0,"z":0},"ready":true,
        "places":[],"visible_cells":(0..7).map(|x| serde_json::json!({
            "key":format!("cell-{x}"),"position":{"x":x,"y":0,"z":0},
            "wall":false,"material":"stone","place_hint":x==1 || x==6,
            "stairs_up":false,"stairs_down":false
        })).collect::<Vec<_>>(),
        "ground_items":[
            {"supply_reachable":false,"reachable":true,"item":{"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(1),"name":"copper token","description":"A small copper disc."},"position":{"x":0,"y":0,"z":0}},
            {"supply_reachable":false,"reachable":false,"item":{"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(2),"name":"stone tablet","description":"A weathered slab of stone."},"position":{"x":6,"y":0,"z":0}}
        ],"inventory":[],"visible_actors":[],
        "combat":{"own_stats":tor_client_common::test_support::stats(),"public_plan":null,"hp":50,"max_hp":50,"sequence":null,"preparation_remaining":null,"preparation_active":false,
            "recovery_remaining":"0","actors":[],"events":[],"objective":null,
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
        id: super::actor_target(id),
        position: Position { x, y: 0, z: 0 },
        asset: None,
    }
}

fn take(mut s: StateView, item: ItemTarget) -> StateView {
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
                supply_reachable: false,
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
            item: super::item_target(2),
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
async fn equipment_goal_sends_matching_socket_and_tells_preparation() {
    let mut initial = state();
    let mut ring = initial.observation.ground_items.remove(0).item;
    ring.name = "silver ring".into();
    ring.class = ItemClass::Ring;
    let item = ring.id;
    initial.observation.inventory.push(ring);
    initial.observation.interactions = Some(InteractionView {
        completed: Vec::new(),
        slots: vec![
            equipment_slot("ring", "on a finger", false),
            equipment_slot("ring", "on a finger", false),
        ],
        preparation: None,
        inventory: vec![ItemInteractionView {
            item,
            slot: Some(equipment_slot("ring", "on a finger", false)),
            equipped_slot: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
        }],
    });
    let mut link = Scripted::new(initial, move |request, now| {
        assert!(is_act(request, &Action::Equip { item, slot: 0 }));
        let mut after = now.clone();
        after.observation.interactions.as_mut().unwrap().inventory[0].equipped_slot = Some(0);
        after
            .observation
            .interactions
            .as_mut()
            .unwrap()
            .completed
            .push(Action::Equip { item, slot: 0 });
        vec![
            Frame::View(
                after,
                Some(Event::ItemStarted {
                    action: Action::Equip { item, slot: 0 },
                }),
            ),
            Frame::Ack,
        ]
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "wear silver ring").await,
        "You begin to equip the silver ring. You finish equipping the silver ring."
    );
    assert_eq!(link.sent.len(), 1);
    assert_eq!(
        play(&mut link, &mut engine, "wear silver ring").await,
        "That item is already equipped."
    );
    assert_eq!(link.sent.len(), 1, "refusal spends no turn");
}

#[tokio::test]
async fn ability_goals_submit_once_from_the_current_position_and_tell_preparation() {
    let target = super::actor_target(42);
    for (verb, ability, technique) in [
        (
            "powerstrike",
            "power_strike",
            tor_client_common::test_support::ability("power_strike"),
        ),
        (
            "magic bolt",
            "magic_bolt",
            tor_client_common::test_support::ability("magic_bolt"),
        ),
        (
            "fear",
            "fear",
            tor_client_common::test_support::ability("fear"),
        ),
    ] {
        let mut initial = state();
        initial.observation.combat = Some(super::creature_combat(vec![
            tor_client_common::test_support::ability("basic_melee"),
            technique,
        ]));
        initial.observation.visible_actors.push(ActorView {
            asset: None,
            id: target,
            name: "goblin".into(),
            description: String::new(),
            position: Position { x: 6, y: 0, z: 0 },
        });
        let mut link = Scripted::new(initial, move |request, now| {
            assert!(
                is_act(
                    request,
                    &Action::UseAbility {
                        id: ability.into(),
                        invocation: tor_protocol::AbilityInvocation::ActorTarget { target },
                        supplies: Default::default(),
                    }
                ),
                "ability invocation must not walk toward the target"
            );
            vec![
                Frame::View(
                    now.clone(),
                    Some(Event::AbilityStarted {
                        ability: tor_client_common::test_support::content(ability),
                        target,
                    }),
                ),
                Frame::Ack,
            ]
        });
        let mut engine = Engine::default();
        let text = play(&mut link, &mut engine, &format!("{verb} goblin")).await;
        assert!(
            text.contains(&format!(
                "You prepare to use {}.",
                tor_client_common::test_support::content(ability).name
            )),
            "{text}"
        );
        assert_eq!(link.sent.len(), 1);
        assert_eq!(link.client.state().observation.position.x, 0);
    }
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

/// Travel has ended, but the character is still recovering when play yields.
/// Keep the real server's travel/permission/wait ordering, including admission
/// being enabled before the character's next turn.
fn yielded_journey(there: StateView, phase: TravelPhase, on: Waiting) -> Vec<Frame> {
    vec![
        Frame::Journey(TravelPhase::Active),
        Frame::Readiness(Readiness {
            revision: 1,
            admission: false,
            resume: vec![],
            cancel: vec![],
        }),
        Frame::Ack,
        Frame::View(
            not_ready(there),
            Some(Event::Moved {
                direction: Direction::East,
            }),
        ),
        Frame::Journey(phase),
        Frame::Readiness(Readiness {
            revision: 2,
            admission: true,
            resume: vec![],
            cancel: vec![],
        }),
        Frame::Waiting(on),
    ]
}

#[tokio::test]
async fn a_yielded_hazard_keeps_the_journey_purpose_and_stops_the_chain() {
    for on in [Waiting::Others, Waiting::Unclaimed, Waiting::Paused] {
        for (command, expected) in [
            (
                "go east, then wait",
                "You set off east. A ruin scout comes into view to the east, and you stop warily.",
            ),
            (
                "take tablet, then wait",
                "You head toward the stone tablet, intent on picking it up. A ruin scout comes into view to the east, and you stop warily.",
            ),
        ] {
            let mut first = true;
            let mut link = Scripted::new(state(), move |request, now| {
                assert!(first && is_travel(request), "a yielded journey must stop the chain");
                first = false;
                let mut there = east(now.clone(), 1);
                there.observation.visible_actors.push(figure(2, "ruin scout", 4));
                yielded_journey(there, TravelPhase::Hazard, on)
            });
            assert_eq!(play(&mut link, &mut Engine::default(), command).await, expected);
            assert_eq!(link.sent.len(), 1);
            assert!(link.queue.is_empty());
        }
    }
}

#[tokio::test]
async fn a_yielded_arrival_keeps_newcomer_caution_and_stops_the_chain() {
    for on in [Waiting::Others, Waiting::Unclaimed, Waiting::Paused] {
        let mut first = true;
        let mut link = Scripted::new(state(), move |request, now| {
            assert!(
                first && is_travel(request),
                "a yielded arrival must not pick up"
            );
            first = false;
            let mut there = east(now.clone(), 6);
            there.observation.visible_actors.push(figure(2, "rat", 1));
            yielded_journey(there, TravelPhase::Arrived, on)
        });
        let text = play(&mut link, &mut Engine::default(), "take tablet, then wait").await;
        assert_eq!(text.lines().next(), Some("You walk over to the stone tablet, but stop short of picking it up as a rat comes into view to the east."));
        assert!(link.client.state().observation.inventory.is_empty());
        assert_eq!(link.sent.len(), 1);
        assert!(link.queue.is_empty());
    }
}

#[tokio::test]
async fn a_yielded_arrival_never_starts_another_leg_or_pickup() {
    for command in ["go east, then wait", "take tablet, then wait"] {
        let mut link = open_ground(usize::MAX);
        if command.starts_with("take") {
            link = Scripted::new(state(), obliging);
        }
        let mut respond = link.respond;
        let mut first = true;
        link.respond = Box::new(move |request, now| {
            assert!(
                first && is_travel(request),
                "arrival cannot bypass WaitingOthers"
            );
            first = false;
            let there = respond(request, now)
                .into_iter()
                .rev()
                .find_map(|frame| match frame {
                    Frame::View(there, _) => Some(there),
                    _ => None,
                })
                .unwrap();
            yielded_journey(there, TravelPhase::Arrived, Waiting::Others)
        });
        let text = play(&mut link, &mut Engine::default(), command).await;
        assert!(!text.contains("pick"), "the pickup did not happen: {text}");
        assert_eq!(link.sent.len(), 1);
        assert!(link.queue.is_empty());
    }
}

#[tokio::test]
async fn a_yielded_blocked_journey_does_not_attack_a_reachable_figure() {
    let mut start = state();
    start
        .observation
        .visible_actors
        .push(figure(2, "ruin scout", 4));
    let mut first = true;
    let mut link = Scripted::new(start, move |request, now| {
        assert!(
            first && is_travel(request),
            "a yielded journey must not attack"
        );
        first = false;
        yielded_journey(east(now.clone(), 3), TravelPhase::Blocked, Waiting::Others)
    });
    let text = play(&mut link, &mut Engine::default(), "attack scout, then wait").await;
    assert!(text.contains("ruin scout bars the way"), "{text}");
    assert_eq!(link.sent.len(), 1);
    assert!(link.queue.is_empty());
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
                    action: Action::UseAbility { .. },
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
                    Some(Event::AbilityStarted {
                        ability: tor_client_common::test_support::content("basic_melee"),
                        target: super::actor_target(2),
                    }),
                ),
                Frame::Ack,
                Frame::View(
                    combat(
                        now.clone(),
                        46,
                        vec![
                            CombatEventView::Ability {
                                strike: None,
                                ability: tor_client_common::test_support::content("basic_melee"),
                                caster: Some(super::actor_target(2)),
                                target: Some(super::actor_target(1)),
                                outcome: AbilityOutcome::Applied,
                            },
                            CombatEventView::Interrupted {
                                actor: super::actor_target(1),
                            },
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
                    CombatEventView::Ability {
                        strike: None,
                        ability: tor_client_common::test_support::content("basic_melee"),
                        caster: Some(super::actor_target(1)),
                        target: Some(super::actor_target(2)),
                        outcome: AbilityOutcome::Applied,
                    },
                    CombatEventView::Died {
                        actor: super::actor_target(2),
                    },
                ],
            );
            after.observation.visible_actors.clear();
            vec![
                Frame::Ack,
                Frame::View(
                    after,
                    Some(Event::AbilityStarted {
                        ability: tor_client_common::test_support::content("basic_melee"),
                        target: super::actor_target(2),
                    }),
                ),
            ]
        }
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "attack scout").await,
        "You prepare to use Basic Melee. The ruin scout uses Basic Melee on you. Your attack is interrupted.\nHP 46/50"
    );
    assert_eq!(
        play(&mut link, &mut engine, "kill it").await,
        "You prepare to use Basic Melee. You use Basic Melee on the ruin scout. It falls dead."
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
    silver.item.id = super::item_target(3);
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
async fn gameplay_chains_wait_for_disclosed_permissions_after_action_and_travel() {
    for travel in [false, true] {
        let mut first = true;
        let mut link = Scripted::new(state(), move |request, now| {
            if !first {
                let Request::Command { context, .. } = request else {
                    panic!("gameplay command required")
                };
                assert_eq!(
                    context.readiness_revision, 2,
                    "next command must wait for the permission boundary after the first effect"
                );
                return obliging(request, now);
            }
            first = false;
            let disabled = Frame::Readiness(Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: if travel {
                    vec![]
                } else {
                    vec![IntentionId("intent-r1".into())]
                },
            });
            let enabled = Frame::Readiness(Readiness {
                revision: 2,
                admission: true,
                resume: vec![],
                cancel: vec![],
            });
            if travel {
                assert!(is_travel(request));
                vec![
                    Frame::Journey(TravelPhase::Active),
                    disabled,
                    Frame::Ack,
                    Frame::View(
                        east(now.clone(), 6),
                        Some(Event::Moved {
                            direction: Direction::East,
                        }),
                    ),
                    Frame::Journey(TravelPhase::Arrived),
                    enabled,
                ]
            } else {
                assert!(matches!(
                    request,
                    Request::Command {
                        command: Command::Act {
                            action: Action::Take { item, .. },
                            ..
                        },
                        ..
                    } if *item == super::item_target(1)
                ));
                vec![
                    Frame::Intention(IntentionPhase::Queued),
                    disabled,
                    Frame::Admitted,
                    Frame::View(
                        take(now.clone(), super::item_target(1)),
                        Some(Event::Taken {
                            item: super::item_target(1),
                            result: super::item_target(1),
                            quantity: 1,
                        }),
                    ),
                    Frame::Intention(IntentionPhase::Resolved),
                    enabled,
                ]
            }
        });
        let mut engine = Engine::default();
        play(
            &mut link,
            &mut engine,
            if travel {
                "get tablet"
            } else {
                "take token, then drop token"
            },
        )
        .await;
        assert_eq!(link.sent.len(), 2);
        assert_eq!(link.client.readiness().revision, 2);
    }
}

#[tokio::test]
async fn timed_attack_finishes_its_lifecycle_before_chained_pickup_or_refusal() {
    for ready_before_resolution in [true, false] {
        for refuse_pickup in [false, true] {
            let mut initial = state();
            initial
                .observation
                .visible_actors
                .push(figure(2, "ruin guard", 1));
            let mut first = true;
            let mut link = Scripted::new(initial, move |request, now| {
                if !first {
                    let Request::Command { context, .. } = request else {
                        panic!("gameplay command required")
                    };
                    assert_eq!(
                        context.readiness_revision, 3,
                        "pickup must use permissions after attack resolution"
                    );
                    return if refuse_pickup {
                        vec![Frame::Reject(ErrorCode::InvalidAction)]
                    } else {
                        obliging(request, now)
                    };
                }
                first = false;
                assert!(matches!(
                    request,
                    Request::Command {
                        command: Command::Act {
                            action: Action::UseAbility { .. },
                            ..
                        },
                        ..
                    }
                ));
                let mut after = combat(
                    now.clone(),
                    50,
                    vec![CombatEventView::Died {
                        actor: super::actor_target(2),
                    }],
                );
                after.observation.visible_actors.clear();
                after.observation.ready = ready_before_resolution;
                let mut frames = vec![
                    Frame::Intention(IntentionPhase::Queued),
                    Frame::Readiness(Readiness {
                        revision: 1,
                        admission: false,
                        resume: vec![],
                        cancel: vec![IntentionId("intent-r1".into())],
                    }),
                    Frame::Admitted,
                    Frame::View(
                        not_ready(now.clone()),
                        Some(Event::AbilityStarted {
                            ability: tor_client_common::test_support::content("basic_melee"),
                            target: super::actor_target(2),
                        }),
                    ),
                    Frame::Intention(IntentionPhase::Started),
                    Frame::Readiness(Readiness {
                        revision: 2,
                        admission: true,
                        resume: vec![],
                        cancel: vec![IntentionId("intent-r1".into())],
                    }),
                    Frame::View(after.clone(), None),
                    Frame::Intention(IntentionPhase::Resolved),
                    Frame::Readiness(Readiness {
                        revision: 3,
                        admission: true,
                        resume: vec![],
                        cancel: vec![],
                    }),
                ];
                if !ready_before_resolution {
                    after.observation.ready = true;
                    frames.push(Frame::View(after, None));
                }
                frames
            });
            let mut engine = Engine::default();
            let text = play(
                &mut link,
                &mut engine,
                "attack ruin guard, then take token, then drop token",
            )
            .await;
            assert!(text.contains("falls dead"), "{text}");
            if refuse_pickup {
                assert!(text.contains("can't pick that up"), "{text}");
                assert_eq!(
                    link.sent.len(),
                    2,
                    "genuine refusal stops the chain without retrying"
                );
                assert!(link.client.state().observation.inventory.is_empty());
            } else {
                assert!(text.contains("pick"), "{text}");
                assert!(text.contains("drop"), "{text}");
                assert_eq!(link.sent.len(), 3);
            }
            assert!(
                link.queue.is_empty(),
                "no attack lifecycle or permission frame left unread"
            );
        }
    }
}

#[tokio::test]
async fn ordinary_action_silence_before_final_permissions_is_uncertain() {
    let mut link = Scripted::new(state(), |_, now| {
        vec![
            Frame::Intention(IntentionPhase::Queued),
            Frame::Readiness(Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![IntentionId("intent-r1".into())],
            }),
            Frame::Admitted,
            Frame::View(
                take(now.clone(), super::item_target(1)),
                Some(Event::Taken {
                    item: super::item_target(1),
                    result: super::item_target(1),
                    quantity: 1,
                }),
            ),
            Frame::Intention(IntentionPhase::Resolved),
        ]
    });
    let error = Engine::default()
        .play(&mut link, "take token")
        .await
        .expect_err("silence must not settle an action without its final permissions");
    assert!(error
        .to_string()
        .contains("may have completed; reconnect and check history"));
    assert_eq!(
        link.sent.len(),
        1,
        "an uncertain action must never be retried"
    );
}

#[tokio::test]
async fn ordinary_journey_silence_before_final_permissions_is_uncertain() {
    let mut first = true;
    let mut link = Scripted::new(state(), move |_, now| {
        if !first {
            return vec![];
        }
        first = false;
        vec![
            Frame::Journey(TravelPhase::Active),
            Frame::Readiness(Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![],
            }),
            Frame::Ack,
            Frame::View(
                east(now.clone(), 6),
                Some(Event::Moved {
                    direction: Direction::East,
                }),
            ),
            Frame::Journey(TravelPhase::Arrived),
        ]
    });
    let error = Engine::default()
        .play(&mut link, "take tablet")
        .await
        .expect_err("silence must not settle arrival without its final permissions");
    assert!(error
        .to_string()
        .contains("may have completed; reconnect and check history"));
    assert_eq!(
        link.sent.len(),
        1,
        "an uncertain arrival cannot submit the pickup"
    );
}

#[tokio::test]
async fn failed_action_waits_for_permission_boundary_before_returning_control() {
    let mut link = Scripted::new(state(), |request, _| {
        assert!(matches!(
            request,
            Request::Command {
                command: Command::Act { .. },
                ..
            }
        ));
        vec![
            Frame::Intention(IntentionPhase::Queued),
            Frame::Readiness(Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![IntentionId("intent-r1".into())],
            }),
            Frame::Admitted,
            Frame::Intention(IntentionPhase::Failed),
            Frame::Readiness(Readiness {
                revision: 2,
                admission: true,
                resume: vec![],
                cancel: vec![],
            }),
        ]
    });
    let mut engine = Engine::default();
    play(&mut link, &mut engine, "take token, then drop token").await;
    assert_eq!(
        link.sent.len(),
        1,
        "execution failure still stops the chain"
    );
    assert_eq!(
        link.client.readiness().revision,
        2,
        "returning control must consume the permission update after failure"
    );
    assert!(!link.client.has_pending_intention());
}

#[tokio::test]
async fn known_execution_failure_is_not_success_when_permission_delivery_times_out() {
    for (phase, waiting) in [
        (IntentionPhase::Failed, false),
        (IntentionPhase::Cancelled, false),
        (IntentionPhase::Failed, true),
        (IntentionPhase::Cancelled, true),
    ] {
        let mut first = true;
        let mut link = Scripted::new(state(), move |_, _| {
            assert!(first, "a known failed action must stop the command chain");
            first = false;
            let mut frames = vec![
                Frame::Intention(IntentionPhase::Queued),
                Frame::Readiness(Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
                Frame::Admitted,
                Frame::Intention(phase),
            ];
            if waiting {
                frames.push(Frame::Waiting(Waiting::Others));
            }
            frames
        });
        let error = Engine::default()
            .play(&mut link, "take token, then wait")
            .await
            .expect_err("a failed action with missing permissions cannot offer stale input");
        assert_eq!(error.to_string(), "The action failed, but the server did not finish updating input permissions. Reconnect before acting again.");
        assert_eq!(link.sent.len(), 1);
        assert_eq!(link.client.state(), &state());
    }
}

#[tokio::test]
async fn failed_or_cancelled_started_work_consumes_permissions_before_the_next_manual_command() {
    for (phase, admission_enabled) in [
        (IntentionPhase::Failed, true),
        (IntentionPhase::Cancelled, true),
        (IntentionPhase::Failed, false),
        (IntentionPhase::Cancelled, false),
    ] {
        let mut first = true;
        let mut link = Scripted::new(state(), move |request, now| {
            if !first {
                let Request::Command { context, .. } = request else {
                    panic!("gameplay command required")
                };
                assert_eq!(context.readiness_revision, 3);
                return obliging(request, now);
            }
            first = false;
            vec![
                Frame::Intention(IntentionPhase::Queued),
                Frame::Readiness(Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
                Frame::Admitted,
                Frame::View(not_ready(now.clone()), None),
                Frame::Intention(IntentionPhase::Started),
                Frame::Readiness(Readiness {
                    revision: 2,
                    admission: true,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
                Frame::View(now.clone(), None),
                Frame::Intention(phase),
                Frame::Readiness(Readiness {
                    revision: 3,
                    admission: admission_enabled,
                    resume: vec![],
                    cancel: vec![],
                }),
            ]
        });
        let mut engine = Engine::default();
        assert_eq!(
            play(&mut link, &mut engine, "take token").await,
            "You can't pick that up from here."
        );
        assert!(link.queue.is_empty());
        assert_eq!(link.client.can_admit_intention(), admission_enabled);
        if admission_enabled {
            play(&mut link, &mut engine, "wait").await;
            assert_eq!(link.sent.len(), 2);
        } else {
            assert_eq!(link.sent.len(), 1);
        }
    }
}

#[tokio::test]
async fn waiting_for_another_actor_never_completes_a_pending_pickup_or_chains_input() {
    for on in [Waiting::Others, Waiting::Unclaimed, Waiting::Paused] {
        let mut first = true;
        let mut link = Scripted::new(state(), move |_, _| {
            assert!(
                first,
                "waiting for another actor must stop the command chain"
            );
            first = false;
            vec![
                Frame::Intention(IntentionPhase::Queued),
                Frame::Readiness(Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
                Frame::Admitted,
                Frame::Waiting(on),
            ]
        });
        let text = play(&mut link, &mut Engine::default(), "take token, then wait").await;
        assert!(
            !text.contains("pick"),
            "a queued pickup has not happened: {text}"
        );
        assert_eq!(link.sent.len(), 1);
        assert!(link.client.has_pending_intention());
        assert!(link.queue.is_empty());
    }
}

#[tokio::test]
async fn resumed_play_explains_waiting_when_no_effects_were_observed() {
    for (on, explanation) in [
        (Waiting::Others, "Someone else is to act first."),
        (
            Waiting::Unclaimed,
            "Another character is to act first, and no one is controlling it.",
        ),
        (Waiting::Stopped, "Nothing more can happen in this game."),
    ] {
        let mut link = Scripted::new(not_ready(state()), move |request, _| {
            assert!(matches!(request, Request::Continue));
            vec![Frame::Ack, Frame::Waiting(on)]
        });
        assert_eq!(
            play(&mut link, &mut Engine::default(), "wait, then take token").await,
            explanation
        );
        assert_eq!(link.sent.len(), 1);
    }
}

#[tokio::test]
async fn started_or_paused_work_waits_without_running_the_remaining_command_chain() {
    for paused in [false, true] {
        let mut first = true;
        let mut link = Scripted::new(state(), move |_, now| {
            assert!(
                first,
                "paid work waiting for another actor must not start another request"
            );
            first = false;
            let mut frames = vec![
                Frame::Intention(IntentionPhase::Queued),
                Frame::Readiness(Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
                Frame::Admitted,
                Frame::View(not_ready(now.clone()), Some(Event::Waited)),
                Frame::Intention(IntentionPhase::Started),
                Frame::Readiness(Readiness {
                    revision: 2,
                    admission: true,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
            ];
            if paused {
                frames.extend([
                    Frame::Intention(IntentionPhase::Paused),
                    Frame::Readiness(Readiness {
                        revision: 3,
                        admission: true,
                        resume: vec![IntentionId("intent-r1".into())],
                        cancel: vec![IntentionId("intent-r1".into())],
                    }),
                ]);
            }
            frames.push(Frame::Waiting(Waiting::Others));
            frames
        });
        assert_eq!(
            play(&mut link, &mut Engine::default(), "wait, then take token").await,
            "Time passes."
        );
        assert_eq!(link.sent.len(), 1);
        assert_eq!(link.client.intentions().len(), 1);
        assert!(link.queue.is_empty());
    }
}

#[tokio::test]
async fn execution_failure_after_admission_stops_the_command_chain() {
    let mut link = Scripted::new(state(), |request, now| match request {
        Request::Command {
            command:
                Command::Act {
                    action: Action::Take { .. },
                    ..
                },
            ..
        } => vec![
            Frame::Intention(IntentionPhase::Queued),
            Frame::Readiness(Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![IntentionId("intent-r1".into())],
            }),
            Frame::Admitted,
            Frame::Intention(IntentionPhase::Failed),
            Frame::Readiness(Readiness {
                revision: 2,
                admission: true,
                resume: vec![],
                cancel: vec![],
            }),
            Frame::Waiting(Waiting::You),
        ],
        other => obliging(other, now),
    });
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "take token, then take tablet").await,
        "You can't pick that up from here."
    );
    assert_eq!(link.sent.len(), 1);
    assert!(!link.client.has_pending_intention());
}

#[tokio::test]
async fn simple_pickups_in_a_row_are_one_sentence_and_again_repeats() {
    let mut start = state();
    let mut twin = start.observation.ground_items[0].clone();
    twin.item.id = super::item_target(3);
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
            state: ({
                let mut a = after.clone();
                a.revision = 1;
                a
            })
            .into(),
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
async fn unavailable_actions_are_refused_without_sending_work() {
    let mut link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    for (line, answer) in [
        ("wear the token", "You aren't carrying that item."),
        ("eat lamp", "You aren't carrying any lamp."),
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
        serde_json::from_value::<GroundItemView>(serde_json::json!({"supply_reachable":false,
            "reachable": true, "position": {"x": 0, "y": 0, "z": 0},
            "item": {"known_supply":null,"quantity": quantity.to_string(), "class":"misc","appearance": "item", "identified": true,
                "id": super::item_target(id), "name": name, "description": ""}}))
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
                    action: Action::UseAbility { .. },
                    ..
                },
            ..
        } => {
            let mut after = combat(
                now.clone(),
                50,
                vec![
                    CombatEventView::Ability {
                        strike: None,
                        ability: tor_client_common::test_support::content("basic_melee"),
                        caster: Some(super::actor_target(1)),
                        target: Some(super::actor_target(2)),
                        outcome: AbilityOutcome::Applied,
                    },
                    CombatEventView::Died {
                        actor: super::actor_target(2),
                    },
                ],
            );
            after.observation.visible_actors.clear();
            vec![
                Frame::Ack,
                Frame::View(
                    after,
                    Some(Event::AbilityStarted {
                        ability: tor_client_common::test_support::content("basic_melee"),
                        target: super::actor_target(2),
                    }),
                ),
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
                    serde_json::from_value(serde_json::json!({"supply_reachable":false,
                        "reachable": false, "position": {"x": 5, "y": 0, "z": 0},
                        "item": {"known_supply":null,"quantity": "1", "class":"misc","appearance": "item", "identified": true,
                            "id":super::item_target(7), "name": "pebble", "description": ""}}))
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

#[tokio::test]
async fn a_count_from_alike_stacks_is_some_of_them_not_the_ones() {
    // Regression: with a stack of two among 17 arrows carried, "drop 2
    // arrows" said "You drop the two arrows".
    let mut s = state();
    let arrows = |id: u64, quantity: u64| {
        serde_json::from_value::<ItemView>(serde_json::json!({"known_supply":null,
            "quantity": quantity.to_string(), "class":"misc","appearance": "item", "identified": true,
            "id": super::item_target(id), "name": "arrow", "description": ""}))
        .unwrap()
    };
    s.observation.inventory = vec![arrows(20, 2), arrows(21, 15)];
    let mut link = Scripted::new(s, obliging);
    let mut engine = Engine::default();
    assert_eq!(
        play(&mut link, &mut engine, "drop 2 arrows").await,
        "You drop two arrows."
    );
}

#[tokio::test]
async fn a_move_told_between_turns_describes_where_it_arrives() {
    // Regression: a spectator saw "You move east." but never the room the
    // watched character walked into.
    let mut link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    engine.welcome(&link);
    let before = link.client.state().clone();
    let mut after = east(before.clone(), 6);
    after.revision = 1;
    let message = link.update(
        60,
        UpdateBody::Observation {
            state: after.into(),
            event: None,
        },
    );
    if let ServerMessage::Update { update } = &message {
        link.client.apply(*update.clone()).unwrap();
    }
    let beats = engine.between_turns(&link, &before, &message);
    let passage = engine.passage(&link, beats);
    assert!(passage.contains("You are in an open"), "{passage}");
}

#[tokio::test]
async fn a_look_after_a_move_in_one_line_describes_the_place_once() {
    // Regression: "go east. look" described the new place in full, then
    // again as the arrival.
    let mut link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    engine.welcome(&link);
    let text = play(&mut link, &mut engine, "go east. look").await;
    assert_eq!(text.matches("You are in").count(), 1, "{text}");
    assert!(!text.contains("You are back in"), "{text}");
}

#[tokio::test]
async fn a_look_before_a_move_still_leaves_the_arrival_told() {
    let mut link = Scripted::new(state(), obliging);
    let mut engine = Engine::default();
    engine.welcome(&link);
    let text = play(&mut link, &mut engine, "look. go east").await;
    assert_eq!(text.matches("You are in").count(), 2, "{text}");
}

/// Context for one synthetic attachment used by this fixture/workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}

#[tokio::test]
async fn ordered_strikes_text_escape_uses_current_permission_and_exact_preparation_identity() {
    for permitted in [true, false] {
        let state = StateView {
            wizard_game: false,
            revision: 0,
            observation: tor_client_common::test_support::observation(),
        };
        let mut link = Scripted::new(state, |_, _| vec![Frame::Ack]);
        let mut snapshot = link.client.snapshot();
        let view = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
        let mut combat = serde_json::to_value(view.combat.as_ref().unwrap()).unwrap();
        combat["preparation_remaining"] = serde_json::json!("5");
        combat["preparation_active"] = serde_json::json!(false);
        combat["sequence"] = serde_json::json!({"completed":1,"total":3,
            "next_strike_remaining":"5","cancel_recovery_remaining":"35"});
        view.combat = Some(serde_json::from_value(combat).unwrap());
        snapshot.readiness.admission = false;
        snapshot.readiness.cancel = if permitted {
            vec![IntentionId("sequence".into())]
        } else {
            vec![]
        };
        snapshot.intentions = vec![IntentionStatus {
            actor: snapshot.actor,
            branch: snapshot.branch.clone(),
            intention: IntentionId("sequence".into()),
            entry_id: EntryId("admitted".into()),
            phase: IntentionPhase::Paused,
        }];
        link.client = ClientState::from_snapshot(snapshot).unwrap();
        let before = link.client.snapshot();
        let output = Engine::default()
            .play(&mut link, "escape west")
            .await
            .unwrap();
        if permitted {
            assert_eq!(link.sent.len(), 1, "{output:?}");
            let value = serde_json::to_value(&link.sent[0]).unwrap();
            assert_eq!(value["command"]["type"], "cancel_intention");
            assert_eq!(value["command"]["intention"], "sequence");
            assert_eq!(value["command"]["step"], "west");
        } else {
            assert!(link.sent.is_empty(), "no permission means no request");
        }
        assert_eq!(
            link.client.snapshot(),
            before,
            "a request spends nothing locally"
        );
    }
}

#[tokio::test]
async fn ordered_strikes_adventure_source_navigation_checks_wizard_authority_before_querying() {
    let mut link = Scripted::new(state(), |_, _| {
        panic!("ordinary role cannot send private detail query")
    });
    let result = Engine::default()
        .play(&mut link, "strikes natural")
        .await
        .unwrap();
    assert!(
        matches!(result, Outcome::Passage(ref text) if text.contains("Wizard authority")),
        "{result:?}"
    );
    assert!(link.sent.is_empty());
}

fn growth_source_request(link: &Scripted) -> Request {
    link.client.command_request(Command::Act {
        expected_revision: link.client.state().revision,
        action: Action::ChooseGrowthSource {
            basis: Box::new(AdvancementBasis {
                actor: ActorId(1),
                branch: branch(),
                stream: fixture_context().stream,
                reset: 0,
                build_epoch: 0,
            }),
            source: GrowthSource::Racial {},
        },
    })
}

#[tokio::test]
async fn admitted_tool_request_waits_for_execution_and_its_permission_boundary() {
    for phase in [
        IntentionPhase::Started,
        IntentionPhase::Resolved,
        IntentionPhase::Failed,
        IntentionPhase::Cancelled,
        IntentionPhase::Suspended,
    ] {
        let mut link = Scripted::new(state(), move |_, now| {
            vec![
                Frame::Intention(IntentionPhase::Queued),
                Frame::Readiness(Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![IntentionId("intent-r1".into())],
                }),
                Frame::Admitted,
                Frame::View(
                    if phase == IntentionPhase::Started {
                        not_ready(now.clone())
                    } else {
                        now.clone()
                    },
                    None,
                ),
                Frame::Intention(phase),
                Frame::Readiness(Readiness {
                    revision: 2,
                    admission: !matches!(phase, IntentionPhase::Suspended),
                    resume: if phase == IntentionPhase::Suspended {
                        vec![IntentionId("intent-r1".into())]
                    } else {
                        vec![]
                    },
                    cancel: if phase.active() {
                        vec![IntentionId("intent-r1".into())]
                    } else {
                        vec![]
                    },
                }),
            ]
        });
        let request = growth_source_request(&link);
        let done = tor_client_text::engine::turn::run_request(
            &mut link,
            &mut Default::default(),
            &mut Default::default(),
            request,
        )
        .await
        .unwrap();
        assert_eq!(
            link.client.readiness().revision,
            2,
            "the next prompt must not use the queued or pre-permission context: {phase:?}"
        );
        assert_eq!(
            done,
            !matches!(phase, IntentionPhase::Failed | IntentionPhase::Cancelled),
            "failed execution cannot be reported as successful admission"
        );
        assert!(link.queue.is_empty());
        if phase == IntentionPhase::Started {
            assert!(
                !link.client.state().observation.ready,
                "the prompt must not wait out preparation or recovery"
            );
        }
    }
}

#[tokio::test]
async fn admitted_tool_request_can_return_while_waiting_for_another_player() {
    let mut link = Scripted::new(state(), |_, _| {
        vec![
            Frame::Intention(IntentionPhase::Queued),
            Frame::Admitted,
            Frame::Waiting(Waiting::Others),
        ]
    });
    let request = growth_source_request(&link);
    assert!(tor_client_text::engine::turn::run_request(
        &mut link,
        &mut Default::default(),
        &mut Default::default(),
        request,
    )
    .await
    .unwrap());
    assert!(link.queue.is_empty(), "consume the actual waiting boundary");
    assert!(link.client.has_pending_intention());
}

async fn assert_admitted_tool_silence_is_uncertain(executed: bool) {
    let mut link = Scripted::new(state(), move |_, _| {
        let mut frames = vec![
            Frame::Intention(IntentionPhase::Queued),
            Frame::Readiness(Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![IntentionId("intent-r1".into())],
            }),
            Frame::Admitted,
        ];
        if executed {
            frames.push(Frame::Intention(IntentionPhase::Resolved));
        }
        frames
    });
    let request = growth_source_request(&link);
    let error = tor_client_text::engine::turn::run_request(
        &mut link,
        &mut Default::default(),
        &mut Default::default(),
        request,
    )
    .await
    .expect_err("silence cannot settle an incomplete admitted tool request");
    assert_eq!(error.to_string(), "The server did not answer. The action may have completed; reconnect and check history before trying again.");
    assert_eq!(link.sent.len(), 1, "uncertain admission is never retried");
    assert_eq!(link.client.readiness().revision, 1);
    assert_eq!(link.client.has_pending_intention(), !executed);
    assert!(link.queue.is_empty());
}

#[tokio::test]
async fn admitted_tool_silence_before_execution_reports_an_uncertain_outcome() {
    assert_admitted_tool_silence_is_uncertain(false).await;
}

#[tokio::test]
async fn admitted_tool_silence_before_permissions_reports_an_uncertain_outcome() {
    assert_admitted_tool_silence_is_uncertain(true).await;
}

#[tokio::test]
async fn immediate_tool_reply_needs_no_later_execution_or_permission_frame() {
    let mut link = Scripted::new(state(), |_, _| vec![Frame::Ack]);
    assert!(tor_client_text::engine::turn::run_request(
        &mut link,
        &mut Default::default(),
        &mut Default::default(),
        Request::Save,
    )
    .await
    .unwrap());
    assert_eq!(link.sent.len(), 1);
    assert!(link.queue.is_empty());
}
