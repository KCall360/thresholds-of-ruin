//! The message area, the single map and the NetHack-style commands.
use std::sync::Arc;
use tor_client_ascii::messages::Messages;
use tor_client_ascii::{map, render, App, Effect, Input, Key};
use tor_client_common::narration::{Line, Topic};
use tor_client_common::ClientState;
use tor_protocol::*;

fn line(topic: Topic, text: &str) -> Line {
    Line {
        topic,
        text: text.into(),
    }
}

/// A 7x3 room at the player's feet with a floor below and a ceiling above,
/// a scout to the east and the player's own head disclosed as an actor.
fn snapshot() -> Snapshot {
    let mut cells = Vec::new();
    for x in -3..=3 {
        for y in -1..=1 {
            for (z, wall) in [(-1, true), (0, false), (1, false), (2, true)] {
                cells.push(serde_json::json!({
                    "key":format!("{x}:{y}:{z}"),"position":{"x":x,"y":y,"z":z},"wall":wall,
                    "stairs_up":false,"stairs_down":x == -3 && y == 0 && z == 0,"place_hint":false
                }));
            }
        }
    }
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"turns","cursor":{"sequence":"0","tick":"100"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":"5","observation":{
            "actor":"1","self_target":super::actor_target(1),"tick":"100","position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":cells,
            "ground_items":[{"reachable":true,"item":{"quantity":"1","class":"weapon","appearance":"sword","identified":true,"id":super::item_target(7),"name":"sword"},"position":{"x":-1,"y":0,"z":0}}],
            "inventory":[
                {"quantity":"1","class":"potion","appearance":"red potion","identified":false,"id":super::item_target(1),"name":"red potion"},
                {"quantity":"1","class":"armor","appearance":"mail","identified":true,"id":super::item_target(2),"name":"mail"}
            ],
            "visible_actors":[
                {"id":super::actor_target(1),"name":"delver","position":{"x":0,"y":0,"z":1}},
                {"id":super::actor_target(2),"name":"ruin scout","position":{"x":2,"y":0,"z":0}}
            ],
            "combat":{"hp":40,"max_hp":50,"preparation_remaining":null,"preparation_active":false,"recovery_remaining":"0",
                "actors":[{"actor":super::actor_target(2),"hostile":true,"injury":"wounded"}],"events":[],
                "objective":"retrieve_and_return","exit":null,"victory":false,"dead":false,"terminal":false},
            "ready":true
        }}
    }))
    .unwrap()
}

fn app() -> App {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot()).unwrap());
    app.ready();
    app
}

fn view() -> Observation {
    snapshot().state.observation.clone()
}

fn update(app: &mut App, sequence: u64, change: impl FnOnce(&mut Observation)) {
    let initial = snapshot();
    let mut state = Arc::unwrap_or_clone(app.state.as_ref().unwrap().snapshot().state);
    state.revision += 1;
    state.observation.tick += 100;
    change(&mut state.observation);
    let tick = state.observation.tick;
    app.update(StreamUpdate {
        context: StreamContext {
            stream: StreamId("fixture-attachment".into()),
            epoch: 0,
        },
        actor: initial.actor,
        branch: initial.branch.clone(),
        cursor: StreamCursor { sequence, tick },
        body: UpdateBody::Observation {
            state: state.into(),
            event: None,
        },
    })
    .unwrap();
}

#[test]
fn routine_lines_are_left_out_and_kills_read_as_one_sentence() {
    let me = super::actor_target(1);
    let scout = super::actor_target(2);
    let hit = CombatEventView::Attack {
        attacker: Some(me),
        target: Some(scout),
        outcome: AttackOutcome::Hit,
    };
    let mut messages = Messages::default();
    let mut view = view();
    view.ground_items.clear();
    messages.absorb(
        &[
            line(
                Topic::Own(Event::Moved {
                    direction: Direction::East,
                }),
                "You move east.",
            ),
            line(Topic::Pacing, "You can act again."),
            line(
                Topic::LostSight(scout),
                "You can no longer see the ruin scout.",
            ),
            line(Topic::Combat(hit), "You struck the ruin scout."),
            line(
                Topic::Combat(CombatEventView::Died { actor: scout }),
                "The ruin scout died.",
            ),
            line(Topic::Status, "HP 40/50 - Recovering: 16 ticks."),
        ],
        &view,
    );
    assert_eq!(messages.text(), "You kill the ruin scout!");
}

#[test]
fn item_work_reads_as_one_action_and_repeats_are_counted() {
    let mut messages = Messages::default();
    let view = view();
    messages.absorb(&[line(Topic::Own(Event::Waited), "Time passes.")], &view);
    messages.push("You begin to drink the red potion.");
    messages.absorb(
        &[line(Topic::Finished, "You finish drinking the red potion.")],
        &view,
    );
    let miss = CombatEventView::Attack {
        attacker: Some(super::actor_target(2)),
        target: Some(super::actor_target(1)),
        outcome: AttackOutcome::Miss,
    };
    for _ in 0..3 {
        messages.absorb(
            &[line(Topic::Combat(miss), "The ruin scout missed you.")],
            &view,
        );
    }
    assert_eq!(
        messages.text(),
        "You finish drinking the red potion.  The ruin scout missed you. (x3)"
    );
    // Drinking can reveal what a potion was; the start of the work already
    // used its new name.
    messages.begin_turn();
    messages.absorb(
        &[line(
            Topic::Own(Event::ItemStarted {
                action: Action::Drink {
                    item: super::item_target(1),
                },
            }),
            "You begin to drink the potion of poison.",
        )],
        &view,
    );
    messages.absorb(
        &[line(Topic::Finished, "You finish drinking the red potion.")],
        &view,
    );
    assert_eq!(
        messages.text(),
        "You finish drinking the red potion.  It was a potion of poison."
    );
}

#[test]
fn moving_onto_things_says_what_is_here() {
    let mut messages = Messages::default();
    let mut view = view();
    let moved = line(
        Topic::Own(Event::Moved {
            direction: Direction::West,
        }),
        "You move west.",
    );
    view.ground_items[0].position = view.position;
    messages.absorb(std::slice::from_ref(&moved), &view);
    assert_eq!(messages.text(), "You see here a sword.");
    messages.begin_turn();
    view.ground_items.clear();
    view.position = Position { x: -3, y: 0, z: 0 };
    messages.absorb(&[moved], &view);
    assert_eq!(messages.text(), "There is a staircase down here.");
}

#[test]
fn a_creature_is_announced_again_only_after_a_long_absence() {
    let scout = super::actor_target(2);
    let mut messages = Messages::default();
    let mut view = view();
    let noticed = [line(Topic::Noticed(scout), "You notice a ruin scout.")];
    messages.absorb(&noticed, &view);
    assert_eq!(messages.text(), "You notice a ruin scout.");
    messages.begin_turn();
    view.tick += tor_client_ascii::messages::SIGHTING_TICKS - 1;
    messages.absorb(&noticed, &view);
    assert_eq!(messages.text(), "");
    view.tick += tor_client_ascii::messages::SIGHTING_TICKS;
    messages.absorb(&noticed, &view);
    assert_eq!(messages.text(), "You notice a ruin scout.");
    assert_eq!(
        messages.log().collect::<Vec<_>>(),
        ["You notice a ruin scout."]
    );
}

#[test]
fn messages_stay_until_the_next_command_and_then_move_to_the_log() {
    let mut app = app();
    update(&mut app, 1, |o| {
        o.combat.as_mut().unwrap().events = vec![CombatEventView::Attack {
            attacker: Some(super::actor_target(2)),
            target: Some(super::actor_target(1)),
            outcome: AttackOutcome::Hit,
        }]
    });
    // Later updates with nothing to say don't clear what was said.
    update(&mut app, 2, |o| o.combat.as_mut().unwrap().events.clear());
    assert_eq!(app.messages.text(), "The ruin scout struck you.");
    assert!(matches!(
        app.input(Input::Key { key: Key::Up }),
        Effect::Request(_)
    ));
    assert_eq!(app.messages.text(), "");
    assert_eq!(
        app.messages.log().collect::<Vec<_>>(),
        ["The ruin scout struck you."]
    );
    app.ready();
    assert_eq!(
        app.input(Input::Key {
            key: Key::MessageLog
        }),
        Effect::None
    );
    assert_eq!(app.message_log, Some(0));
    assert_eq!(app.input(Input::Key { key: Key::Up }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert_eq!(app.message_log, None);
    render::Canvas::default().draw(&app);
}

#[test]
fn more_holds_keys_until_every_message_is_shown() {
    let mut app = app();
    for i in 0..12 {
        app.messages.push(format!(
            "Message number {i} is long enough to fill much of a row."
        ));
    }
    assert!(app.messages.more());
    let shown = app.messages.shown(true);
    assert_eq!(shown.len(), tor_client_ascii::messages::ROWS);
    assert!(shown.last().unwrap().ends_with("--More--"));
    // A movement key pages instead of moving.
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    assert!(app.messages.shown(true)[0] != shown[0]);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(!app.messages.more());
    assert!(matches!(
        app.input(Input::Key { key: Key::Right }),
        Effect::Request(_)
    ));
    // A spectator can't page, so sees the newest rows and is never held.
    let mut messages = Messages::default();
    for i in 0..12 {
        messages.push(format!(
            "Message number {i} is long enough to fill much of a row."
        ));
    }
    assert!(messages.shown(false).last().unwrap().contains("11"));
}

#[test]
fn the_single_map_merges_heights_and_never_draws_the_player_as_a_creature() {
    let mut snapshot = snapshot();
    let o = &mut Arc::make_mut(&mut snapshot.state).observation;
    let cell = |x: i32, z: i32, wall: bool| {
        serde_json::from_value::<CellView>(serde_json::json!({
            "key":format!("extra:{x}:{z}"),"position":{"x":x,"y":-1,"z":z},"wall":wall,
            "stairs_up":false,"stairs_down":false,"place_hint":false
        }))
        .unwrap()
    };
    // Column -2,-1: solid at the feet, open above: a low wall.
    o.visible_cells.retain(|c| {
        (c.position.x, c.position.y) != (-2, -1) && (c.position.x, c.position.y) != (2, -1)
    });
    o.visible_cells.push(cell(-2, 0, true));
    o.visible_cells.push(cell(-2, 1, false));
    // Column 2,-1: open at the feet and below: a drop.
    o.visible_cells.push(cell(2, 0, false));
    o.visible_cells.push(cell(2, -1, false));
    // A stair's far landing in the player's own column is left out.
    let mut landing = cell(0, -17, false);
    landing.position.y = 0;
    o.visible_cells.push(landing);
    let state = ClientState::from_snapshot(snapshot).unwrap();
    let tiles = render::map_tiles(&state);
    let at = |x: i32, y: i32| {
        tiles
            .iter()
            .find(|t| (t.position.x, t.position.y) == (x, y))
            .unwrap()
    };
    assert_eq!((at(0, 0).glyph, at(0, 0).kind), ('@', map::Kind::Player));
    assert_eq!(tiles.iter().filter(|t| t.glyph == '@').count(), 1);
    assert!(!tiles.iter().any(|t| t.position.z == -17));
    assert_eq!((at(2, 0).glyph, at(2, 0).kind), ('s', map::Kind::Creature));
    assert_eq!(at(2, 0).color, map::creature_color("ruin scout"));
    assert_eq!((at(-1, 0).glyph, at(-1, 0).kind), (')', map::Kind::Item));
    assert_eq!(
        (at(-3, 0).glyph, at(-3, 0).kind),
        ('>', map::Kind::StairsDown)
    );
    assert_eq!(
        (at(-2, -1).glyph, at(-2, -1).kind),
        ('#', map::Kind::LowWall)
    );
    assert_eq!((at(2, -1).glyph, at(2, -1).kind), ('^', map::Kind::Drop));
    assert_eq!((at(1, 1).glyph, at(1, 1).kind), ('.', map::Kind::Floor));
    let columns: std::collections::BTreeSet<_> =
        tiles.iter().map(|t| (t.position.x, t.position.y)).collect();
    assert_eq!(columns.len(), tiles.len());
    assert_eq!(map::creature_glyph("stone guardian"), 'g');
    assert_eq!(map::creature_glyph(""), '&');
}

#[test]
fn letters_stay_with_items_and_choose_from_menus() {
    let mut app = app();
    let o = view();
    let entries = tor_client_ascii::inventory_entries(&app, &o);
    assert_eq!(
        entries.iter().map(|e| e.short()).collect::<Vec<_>>(),
        ["a - a red potion", "b - a mail"]
    );
    // Dropping the potion frees a; the mail keeps b.
    update(&mut app, 1, |o| {
        o.inventory.remove(0);
    });
    assert_eq!(app.letters.values().copied().collect::<Vec<_>>(), ['b']);
    // Drop always asks, and a letter answers.
    assert_eq!(app.input(Input::Key { key: Key::Drop }), Effect::None);
    assert_eq!(app.choice_letter(0), 'b');
    assert!(matches!(
        app.input(Input::Text { text: "b".into() }),
        Effect::Request(Request::Command {
            command: Command::Act {
                action: Action::Drop { item, quantity: None },
                ..
            },
            ..
        }) if item == super::item_target(2)
    ));
    app.ready();
    assert_eq!(
        app.input(Input::Key {
            key: Key::Inventory
        }),
        Effect::None
    );
    assert!(app.inventory_open);
    render::Canvas::default().draw(&app);
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    assert!(!app.inventory_open);
}

#[test]
fn look_names_a_creature_and_travel_jumps_to_stairs_without_time() {
    let mut app = app();
    assert_eq!(app.input(Input::Key { key: Key::Look }), Effect::None);
    app.input(Input::Key { key: Key::Right });
    app.input(Input::Key { key: Key::Right });
    assert_eq!(app.input(Input::Key { key: Key::Look }), Effect::None);
    assert_eq!(app.messages.text(), "s - a ruin scout (hostile, wounded).");
    assert_eq!(app.input(Input::Key { key: Key::Travel }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Descend }), Effect::None);
    assert_eq!(app.travel_cursor.map(|c| (c.x, c.y)), Some((-3, 0)));
    assert!(matches!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::Request(Request::Command {
            command: Command::Travel { destination, .. },
            ..
        }) if destination == "-3:0:0"
    ));
}

#[test]
fn running_refuses_with_a_creature_in_view_and_stops_at_walls() {
    let mut app = app();
    assert_eq!(app.input(Input::Key { key: Key::RunLeft }), Effect::None);
    assert_eq!(app.status, "You can't run with a creature in view.");
    update(&mut app, 1, |o| {
        o.visible_actors.retain(|a| a.id == o.self_target);
        o.ground_items.clear();
    });
    assert!(matches!(
        app.input(Input::Key { key: Key::RunUp }),
        Effect::Request(_)
    ));
    assert_eq!(app.running, Some(Direction::North));
    app.ready();
    // North of the player is floor, so the run continues once settled...
    assert!(matches!(app.continue_run(), Effect::Request(_)));
    app.ready();
    // ...and stops when the way ahead isn't open.
    update(&mut app, 2, |o| {
        o.position = Position { x: 0, y: -1, z: 0 };
    });
    assert_eq!(app.continue_run(), Effect::None);
    assert_eq!(app.running, None);
    // Starting a run into the unknown is refused locally.
    assert_eq!(app.input(Input::Key { key: Key::RunUp }), Effect::None);
    assert_eq!(app.status, "You can't run that way.");
}

#[test]
fn a_refused_move_says_the_way_is_blocked() {
    let mut app = app();
    assert!(matches!(
        app.input(Input::Key { key: Key::Up }),
        Effect::Request(_)
    ));
    app.ready();
    app.refused("You can't do that now.".into());
    assert_eq!(app.status, "You can't go that way.");
    assert!(matches!(
        app.input(Input::Key { key: Key::Wait }),
        Effect::Request(_)
    ));
    app.ready();
    app.refused("You can't do that now.".into());
    assert_eq!(app.status, "You can't do that now.");
}

#[test]
fn refusals_status_lines_and_the_end_screen_use_plain_words() {
    assert_eq!(
        tor_client_ascii::plain_error(ErrorCode::InvalidAction, "Intention is unavailable"),
        "You can't do that now."
    );
    assert_eq!(
        tor_client_ascii::plain_error(ErrorCode::InvalidRequest, "Odd request"),
        "Odd request."
    );
    assert_eq!(
        tor_client_ascii::plain_error(ErrorCode::ControlTaken, "taken"),
        "Another player has control; you can watch until they release it."
    );
    let mut app = app();
    let [first, second] = tor_client_ascii::status_lines(&app, app.state.as_ref().unwrap());
    assert!(first.contains("Retrieve the objective item"));
    assert_eq!(second, "HP:40(50)  T:100");
    update(&mut app, 1, |o| {
        let combat = o.combat.as_mut().unwrap();
        combat.victory = true;
        combat.terminal = true;
    });
    assert!(app.messages.text().contains("Victory! This run has ended."));
    // A run in progress ends with the game, and moves say why they're refused.
    app.running = Some(Direction::East);
    assert_eq!(app.continue_run(), Effect::None);
    assert_eq!(app.running, None);
    assert!(!app.end_dismissed);
    render::Canvas::default().draw(&app);
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.end_dismissed);
    assert_eq!(app.input(Input::Key { key: Key::Left }), Effect::None);
    assert_eq!(app.status, "This run has ended.");
}
