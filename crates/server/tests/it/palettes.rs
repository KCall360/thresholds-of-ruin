//! Asset palettes: observations name each disclosed thing's asset, and each
//! client gets a palette forecast from the themes of the regions near its
//! actor, never from what they hold. See docs/protocol.md#asset-palettes.
use futures_util::{SinkExt, StreamExt};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tor_protocol::*;
use tor_server::journal::Command as Journal;
use tor_server::{scenario_package, serve, Account, Engine, Scenario, Service, Streaming};

const ROOT: &str = "../../scenarios/tests/generated-filler";

/// Two stone halls around two caves (their own theme); the far hall is a
/// vault (another). Radii of zero, so a palette covers one hop around the
/// character.
fn caves() -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let mut scenario = scenario_package::load(&root, 5, None, false).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|v| v.to_string()).collect()
}

fn stone() -> BTreeSet<String> {
    set(&["terrain.floor.stone", "terrain.wall.stone"])
}
fn cave() -> BTreeSet<String> {
    set(&[
        "terrain.floor.cave",
        "terrain.wall.cave",
        "creature.rat",
        "item.coin",
    ])
}
fn vault() -> BTreeSet<String> {
    set(&["terrain.floor.marble", "terrain.wall.marble"])
}
fn union(parts: &[BTreeSet<String>]) -> BTreeSet<String> {
    parts
        .iter()
        .flatten()
        .cloned()
        .chain(["creature.delver".into()])
        .collect()
}

/// Act as the character after any AI turns due first.
fn act(engine: &mut Engine, action: Action) -> Result<(), tor_server::Failure> {
    while let Some((actor, ai)) = engine.next_ai_action() {
        let revision = engine.revision(actor).unwrap();
        engine
            .command(
                "player",
                "test",
                actor,
                &uuid::Uuid::new_v4().to_string(),
                &engine.branch().clone(),
                Journal::Act {
                    expected_revision: revision,
                    action: ai,
                },
            )
            .unwrap();
    }
    let revision = engine.revision(ActorId(1)).unwrap();
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            Journal::Act {
                expected_revision: revision,
                action,
            },
        )
        .map(|_| ())
}

/// Step east along the caves' straight corridor, waiting for rats in the way.
fn east(engine: &mut Engine) {
    for _ in 0..20 {
        let moved = act(
            engine,
            Action::Move {
                direction: Direction::East,
            },
        );
        if moved.is_ok() {
            return;
        }
        act(engine, Action::Wait).unwrap();
    }
    panic!("blocked for good");
}

/// From the start into the first cave, and across it into the second.
const INTO_CAVE: usize = 10;
const ACROSS_CAVE: usize = 24;

#[test]
fn observations_name_each_things_asset_from_the_scenario() {
    let mut engine = Engine::memory(caves()).unwrap();
    let assets = |engine: &Engine| {
        let seen = engine.state(ActorId(1)).unwrap().observation;
        seen.visible_cells
            .iter()
            .filter_map(|c| c.asset.clone())
            .chain(seen.visible_actors.iter().filter_map(|a| a.asset.clone()))
            .chain(
                seen.ground_items
                    .iter()
                    .filter_map(|i| i.item.asset.clone()),
            )
            .collect::<BTreeSet<_>>()
    };
    // From the start only the stone hall's floor is in sight.
    assert_eq!(assets(&engine), set(&["terrain.floor.stone"]));
    let mut seen = BTreeSet::new();
    for _ in 0..INTO_CAVE + ACROSS_CAVE {
        east(&mut engine);
        seen.extend(assets(&engine));
    }
    // Cave terrain, and a rat or coin.
    for asset in ["terrain.floor.cave", "terrain.wall.cave"] {
        assert!(seen.contains(asset), "{asset} in {seen:?}");
    }
    assert!(
        seen.contains("creature.rat") || seen.contains("item.coin"),
        "{seen:?}"
    );
}

#[test]
fn a_palette_forecasts_nearby_themes_never_what_regions_hold() {
    // The same caves with different hidden contents: other counts of rats
    // and coins in the second cave.
    let directory = tempfile::tempdir().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let package = scenario_package::load(&root, 5, None, false)
        .unwrap()
        .package
        .unwrap();
    let mut regions = package.region_defs().unwrap();
    let recipe = regions[2].generate.as_mut().unwrap();
    recipe.actors.as_mut().unwrap().count = [2, 2];
    recipe.items.as_mut().unwrap().count = [3, 3];
    scenario_package::write_package(directory.path(), &package.manifest, &regions).unwrap();
    let mut other = scenario_package::load(directory.path(), 5, None, true).unwrap();
    other.streaming = caves().streaming;

    let mut first = Engine::memory(caves()).unwrap();
    let mut second = Engine::memory(other).unwrap();
    // One hop from the start hall: the hall and the first cave.
    assert_eq!(first.palette(ActorId(1)), Some(union(&[stone(), cave()])));
    for step in 0..INTO_CAVE + ACROSS_CAVE {
        east(&mut first);
        east(&mut second);
        assert_eq!(
            first.palette(ActorId(1)),
            second.palette(ActorId(1)),
            "step {step}"
        );
    }
    // In the second cave: both caves and the vault, and no longer stone.
    assert_eq!(first.palette(ActorId(1)), Some(union(&[cave(), vault()])));
}

#[test]
fn a_game_that_doesnt_stream_gets_the_whole_packages_palette() {
    let mut scenario = caves();
    scenario.streaming = None;
    let engine = Engine::memory(scenario).unwrap();
    assert_eq!(
        engine.palette(ActorId(1)),
        Some(union(&[stone(), cave(), vault()]))
    );
}

#[test]
fn an_actor_shows_its_archetypes_asset_and_validation_checks_that_one() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let temp = tempfile::tempdir().unwrap();
    crate::support::copy_package(&root, temp.path());
    let manifest = temp.path().join("scenario.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    // The rat's appearance pool names a forecast asset, but the rat itself
    // shows creature.bat, which no theme names.
    let edited = text
        .replace(
            "[archetypes.rat]\nasset = \"creature.rat\"",
            "[archetypes.rat]\nasset = \"creature.bat\"\nname = \"rat\"\nappearance_pool = \"furs\"",
        )
        .replace(
            "[assets]",
            "[appearance_pools.furs]\nappearances = [\"grey fur\"]\nasset = \"creature.rat\"\n\n[assets]",
        );
    assert_ne!(edited, text);
    std::fs::write(&manifest, edited).unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("creature.bat"), "{error}");
}

#[test]
fn scenarios_without_assets_have_no_palette() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let engine = Engine::memory(scenario_package::load(&root, 5, None, false).unwrap()).unwrap();
    assert_eq!(engine.palette(ActorId(1)), None);
    let state = engine.state(ActorId(1)).unwrap();
    assert!(state
        .observation
        .visible_cells
        .iter()
        .all(|c| c.asset.is_none()));
}

#[test]
fn a_region_must_forecast_the_assets_it_shows() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(ROOT);
    let temp = tempfile::tempdir().unwrap();
    crate::support::copy_package(&root, temp.path());
    // Rats in the stone hall, whose theme doesn't name their asset.
    let manifest = temp.path().join("scenario.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        text.replace(
            "stone = [\"terrain.floor.stone\", \"terrain.wall.stone\"]",
            "stone = [\"terrain.floor.stone\"]",
        ),
    )
    .unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("terrain.wall.stone"), "{error}");
    std::fs::write(
        &manifest,
        text.replace("asset = \"item.coin\"", "asset = \"Item Coin\""),
    )
    .unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(
        error.message.contains("Invalid asset identifier"),
        "{error}"
    );
    std::fs::write(
        &manifest,
        text.replace("vault = [", "unknown = [\"x\"]\nvault = ["),
    )
    .unwrap();
    let error = scenario_package::validate(temp.path()).unwrap_err();
    assert!(error.message.contains("unknown theme"), "{error}");
}

// ----- Over WebSocket -----

type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn receive(client: &mut Client) -> Option<ServerMessage> {
    let frame = timeout(Duration::from_secs(5), client.next())
        .await
        .ok()??
        .ok()?;
    match frame {
        Message::Text(text) => serde_json::from_str(&text).ok(),
        _ => None,
    }
}

async fn send(client: &mut Client, id: &str, request: Request) {
    let message = ClientMessage::Request {
        request_id: id.into(),
        request,
    };
    client
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

async fn connect(address: &str, token: &str) -> Client {
    let (mut client, _) = connect_async(address).await.unwrap();
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        token: token.into(),
        frontend: "test".into(),
    };
    client
        .send(Message::Text(serde_json::to_string(&hello).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(
        receive(&mut client).await,
        Some(ServerMessage::Welcome { .. })
    ));
    client
}

/// A client's view, kept current from snapshots and updates, with every
/// palette message it got.
struct Viewer {
    client: Client,
    state: StateView,
    branch: BranchId,
    palettes: Vec<(Option<String>, PaletteUpdate)>,
    step: usize,
}

impl Viewer {
    /// Attach, taking the snapshot and the palette that follows it.
    async fn attach(address: &str, token: &str) -> Self {
        let mut client = connect(address, token).await;
        send(&mut client, "attach", Request::Attach { actor: ActorId(1) }).await;
        let Some(ServerMessage::Snapshot { snapshot, .. }) = receive(&mut client).await else {
            panic!("expected a snapshot")
        };
        let mut viewer = Self {
            client,
            state: snapshot.state,
            branch: snapshot.branch,
            palettes: Vec::new(),
            step: 0,
        };
        viewer.until(|v| !v.palettes.is_empty()).await;
        viewer
    }

    fn take(&mut self, message: ServerMessage) -> Option<ServerMessage> {
        match message {
            ServerMessage::Update { update } => {
                match update.body {
                    UpdateBody::Observation { state, .. } => self.state = *state,
                    UpdateBody::ObservationDelta { state, .. } => {
                        self.state = state.apply(&self.state).unwrap()
                    }
                    _ => {}
                }
                None
            }
            ServerMessage::Palette {
                request_id,
                palette,
            } => {
                self.palettes.push((request_id, palette));
                None
            }
            other => Some(other),
        }
    }

    async fn until(&mut self, done: impl Fn(&Self) -> bool) {
        while !done(self) {
            let message = receive(&mut self.client).await.expect("connected");
            self.take(message);
        }
    }

    /// Act, returning the error if it was refused.
    async fn act(&mut self, action: Action) -> Option<ErrorCode> {
        self.step += 1;
        let id = format!("step-{}", self.step);
        let request = Request::Command {
            branch: self.branch.clone(),
            command: Command::Act {
                expected_revision: self.state.revision,
                action,
            },
        };
        send(&mut self.client, &id, request).await;
        loop {
            let message = receive(&mut self.client).await.expect("connected");
            match self.take(message) {
                Some(ServerMessage::Ack { request_id, .. }) if request_id == id => return None,
                Some(ServerMessage::Error {
                    request_id: Some(request_id),
                    code,
                    ..
                }) if request_id == id => return Some(code),
                _ => {}
            }
        }
    }

    /// Step east. The server runs rats between the character's turns, so
    /// wait for the character to be ready; while it's ready, time waits for
    /// it, so a refused step then means a rat is in the way.
    async fn east(&mut self) {
        for _ in 0..200 {
            if !self.state.observation.ready {
                let message = receive(&mut self.client).await.expect("connected");
                self.take(message);
                continue;
            }
            let moved = self
                .act(Action::Move {
                    direction: Direction::East,
                })
                .await;
            if moved.is_none() {
                return;
            }
            if self.state.observation.ready {
                self.act(Action::Wait).await;
            }
        }
        panic!("blocked for good");
    }
}

fn full(update: &PaletteUpdate) -> &BTreeSet<String> {
    match &update.body {
        PaletteBody::Full { assets } => assets,
        body => panic!("expected a full palette, got {body:?}"),
    }
}

#[tokio::test]
async fn clients_get_a_full_palette_then_deltas_and_can_ask_for_it_again() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}", listener.local_addr().unwrap());
    let service = Arc::new(Mutex::new(Service::new(Engine::memory(caves()).unwrap())));
    let accounts = vec![
        Account {
            role: AccessRole::Player,
            user: "alice".into(),
            token: "alice-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
        Account {
            role: AccessRole::Spectator,
            user: "bob".into(),
            token: "bob-test-token".into(),
            actors: BTreeSet::from([ActorId(1)]),
        },
    ];
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(serve(listener, service, accounts, async {
        let _ = stopped.await;
    }));

    // Attaching sends the whole palette, unasked.
    let mut player = Viewer::attach(&address, "alice-test-token").await;
    send(&mut player.client, "control", Request::AcquireControl).await;
    let (request, first) = &player.palettes[0];
    assert_eq!(*request, None);
    assert_eq!(first.revision, 1);
    assert_eq!(*full(first), union(&[stone(), cave()]));
    let mut spectator = Viewer::attach(&address, "bob-test-token").await;
    assert_eq!(*full(&spectator.palettes[0].1), union(&[stone(), cave()]));

    // Walking into the first cave changes nothing; into the second, a delta.
    for _ in 0..INTO_CAVE + ACROSS_CAVE {
        player.east().await;
    }
    player.until(|v| v.palettes.len() >= 2).await;
    assert_eq!(
        player.palettes.len(),
        2,
        "one change: {:?}",
        player.palettes
    );
    let (request, delta) = &player.palettes[1];
    assert_eq!(*request, None);
    assert_eq!(delta.revision, 2);
    assert_eq!(
        delta.body,
        PaletteBody::Delta {
            base: 1,
            added: vault(),
            removed: stone(),
        }
    );
    spectator.until(|v| v.palettes.len() >= 2).await;
    assert_eq!(spectator.palettes[1].1.body, delta.body);

    // Asking gets the whole palette, answering the request. Spectators may ask.
    send(&mut spectator.client, "palette", Request::Palette).await;
    spectator.until(|v| v.palettes.len() >= 3).await;
    let (request, again) = &spectator.palettes[2];
    assert_eq!(request.as_deref(), Some("palette"));
    assert_eq!(again.revision, 3);
    assert_eq!(*full(again), union(&[cave(), vault()]));

    // Reconnecting starts over with the whole palette.
    drop(spectator);
    let reconnected = Viewer::attach(&address, "bob-test-token").await;
    let (_, fresh) = &reconnected.palettes[0];
    assert_eq!(fresh.revision, 1);
    assert_eq!(*full(fresh), union(&[cave(), vault()]));
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}
