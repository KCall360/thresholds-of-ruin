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
        basic_attack: None,
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
                    "trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":x == -3 && y == 0 && z == 0,"lit":true,"place_hint":false
                }));
            }
        }
    }
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"turns","cursor":{"sequence":"0","tick":"100"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":"5","observation":{"linked_holds":[],
            "actor":"1","self_target":super::actor_target(1),"tick":"100","position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":cells,
            "ground_items":[{"presentation":null,"supply_reachable":false,"reachable":true,"item":{"known_currency":null,"known_supply":null,"quantity":"1","class":"weapon","appearance":"sword","identified":true,"id":super::item_target(7),"name":"sword"},"position":{"x":-1,"y":0,"z":0}}],
            "inventory":[
                {"known_currency":null,"known_supply":null,"quantity":"1","class":"potion","appearance":"red potion","identified":false,"id":super::item_target(1),"name":"red potion"},
                {"known_currency":null,"known_supply":null,"quantity":"1","class":"armor","appearance":"mail","identified":true,"id":super::item_target(2),"name":"mail"}
            ],
            "visible_actors":[
                {"id":super::actor_target(1),"name":"delver","position":{"x":0,"y":0,"z":1}},
                {"id":super::actor_target(2),"name":"ruin scout","position":{"x":2,"y":0,"z":0}}
            ],
            "combat":{"public_plan":null,"hp":"40","max_hp":"50","sequence":null,"preparation_remaining":null,"preparation_active":false,"recovery_remaining":"0",
                "actors":[{"actor":super::actor_target(2),"hostile":true,"allied":false,"injury":"wounded"}],"events":[],
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

fn basic_attack_view() -> Observation {
    let mut view = view();
    let mut stats = tor_client_common::test_support::stats();
    let mut attack = tor_client_common::test_support::ability("custom.strike");
    attack.name = "Ordinary attack".into();
    attack.basic_attack = true;
    attack.reach = AbilityReach::Melee;
    attack.weapon_selection = true;
    stats.abilities = vec![attack];
    view.combat.as_mut().unwrap().own_stats = Some(stats);
    view
}

fn own_strike(view: &Observation, outcome: AbilityOutcome) -> CombatEventView {
    CombatEventView::Ability {
        caster: Some(view.self_target),
        target: Some(super::actor_target(2)),
        ability: ContentRef {
            id: "custom.strike".into(),
            name: "Ordinary attack".into(),
        },
        outcome,
        strike: None,
    }
}

#[test]
fn basic_attack_messages_show_only_hit_or_miss_and_keep_full_narration() {
    use tor_client_common::narration::observation_lines;

    for (outcome, expected, full) in [
        (
            AbilityOutcome::Applied,
            "You hit the ruin scout.",
            "You used Ordinary attack on the ruin scout.",
        ),
        (
            AbilityOutcome::Miss,
            "You miss the ruin scout.",
            "You used Ordinary attack on the ruin scout, but missed.",
        ),
    ] {
        let before = basic_attack_view();
        let mut preparing = before.clone();
        preparing.tick += 1;
        let start = Event::AbilityStarted {
            ability: ContentRef {
                id: "custom.strike".into(),
                name: "Ordinary attack".into(),
            },
            target: super::actor_target(2),
        };
        let started = observation_lines(&before, &preparing, Some(&start));
        assert_eq!(started[0].text, "You prepare to use Ordinary attack.");
        assert_eq!(started[0].topic, Topic::Own(start));
        let mut messages = Messages::default();
        messages.absorb(&started, &preparing);
        assert_eq!(messages.text(), "", "routine preparation should be quiet");

        let mut after = preparing.clone();
        after.tick += 1;
        let hit = own_strike(&after, outcome);
        after.combat.as_mut().unwrap().events = vec![hit.clone()];
        let resolved = observation_lines(&preparing, &after, None);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].text, full);
        assert_eq!(resolved[0].topic, Topic::Combat(hit.clone()));
        messages.absorb(&resolved, &after);
        assert_eq!(messages.text(), expected);
        assert_eq!(after.combat.as_ref().unwrap().events, [hit]);
        messages.begin_turn();
        assert_eq!(messages.log().collect::<Vec<_>>(), [expected]);
    }
}

#[test]
fn streamed_basic_attack_keeps_history_and_stays_quiet_until_its_result() {
    let mut initial = snapshot();
    Arc::make_mut(&mut initial.state).observation = basic_attack_view();
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(initial.clone()).unwrap());
    let mut preparing = Arc::unwrap_or_clone(initial.state.clone());
    preparing.revision += 1;
    preparing.observation.tick += 1;
    preparing.observation.ready = false;
    let combat = preparing.observation.combat.as_mut().unwrap();
    combat.preparation_remaining = Some(10);
    combat.preparation_active = true;
    let target = super::actor_target(2);
    let started = HistoryEntry {
        id: EntryId("basic-attack-start".into()),
        branch: initial.branch.clone(),
        actor: initial.actor,
        tick: preparing.observation.tick,
        author: Author::User {
            user: "test".into(),
        },
        audience: Audience::Actor,
        content: HistoryContent::Action {
            action: Action::UseAbility {
                id: "custom.strike".into(),
                invocation: AbilityInvocation {
                    aim: AbilityAim::ActorTarget { target },
                    weapon: WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
            event: Box::new(Event::AbilityStarted {
                ability: ContentRef {
                    id: "custom.strike".into(),
                    name: "Ordinary attack".into(),
                },
                target,
            }),
        },
    };
    app.update(StreamUpdate {
        context: initial.context.clone(),
        actor: initial.actor,
        branch: initial.branch.clone(),
        cursor: StreamCursor {
            sequence: 1,
            tick: preparing.observation.tick,
        },
        body: UpdateBody::Observation {
            state: preparing.into(),
            event: Some(Box::new(started.clone())),
        },
    })
    .unwrap();
    assert_eq!(app.messages.text(), "");
    assert!(app
        .state
        .as_ref()
        .unwrap()
        .narration()
        .iter()
        .any(|line| line == "You prepare to use Ordinary attack."));

    update(&mut app, 2, |view| {
        let strike = own_strike(view, AbilityOutcome::Applied);
        view.ready = true;
        let combat = view.combat.as_mut().unwrap();
        combat.preparation_remaining = None;
        combat.preparation_active = false;
        combat.events = vec![strike];
    });
    assert_eq!(app.messages.text(), "You hit the ruin scout.");
    let state = app.state.as_ref().unwrap();
    assert_eq!(state.history(), [started]);
    assert!(state
        .narration()
        .iter()
        .any(|line| line == "You used Ordinary attack on the ruin scout."));
    update(&mut app, 3, |view| {
        view.combat.as_mut().unwrap().events.clear()
    });
    assert_eq!(app.messages.text(), "You hit the ruin scout.");
}

#[test]
fn basic_attack_messages_keep_each_strike_and_the_disclosed_departed_target() {
    let mut before = basic_attack_view();
    before.visible_actors[1].name = "ruin\n scout".into();
    let mut after = before.clone();
    after.tick += 1;
    after
        .visible_actors
        .retain(|actor| actor.id == after.self_target);
    // A last strike can remove the granted ability together with the target.
    after.combat.as_mut().unwrap().own_stats = None;
    let mut first = own_strike(&before, AbilityOutcome::Applied);
    let mut second = first.clone();
    if let CombatEventView::Ability { strike, .. } = &mut first {
        *strike = Some(0);
    }
    if let CombatEventView::Ability { strike, .. } = &mut second {
        *strike = Some(1);
    }
    after.combat.as_mut().unwrap().events = vec![
        first.clone(),
        second.clone(),
        CombatEventView::Died {
            actor: super::actor_target(2),
        },
    ];
    let lines = tor_client_common::narration::observation_lines(&before, &after, None);
    let mut messages = Messages::default();
    messages.absorb(&lines, &after);
    assert_eq!(
        messages.text(),
        "You hit the ruin scout. (x2)  The ruin scout died."
    );
    assert!(lines.iter().any(|line| {
        line.topic == Topic::Combat(second.clone())
            && line.text == "You used Ordinary attack (strike 2) on the ruin scout."
    }));
    assert_eq!(after.combat.as_ref().unwrap().events[0], first);
}

#[test]
fn concise_combat_never_infers_anonymous_names_or_unknown_ability_semantics() {
    let before = basic_attack_view();
    let mut after = before.clone();
    after.tick += 1;
    for (caster, target, id, name, outcome, expected) in [
        (
            Some(before.self_target),
            None,
            "custom.strike",
            "Ordinary attack",
            AbilityOutcome::Applied,
            "You hit something.",
        ),
        (
            Some(before.self_target),
            Some(super::actor_target(99)),
            "custom.strike",
            "Ordinary attack",
            AbilityOutcome::Miss,
            "You miss the figure.",
        ),
        (
            Some(before.self_target),
            Some(before.self_target),
            "custom.strike",
            "Ordinary attack",
            AbilityOutcome::Applied,
            "You hit yourself.",
        ),
        (
            None,
            Some(before.self_target),
            "custom.strike",
            "Ordinary attack",
            AbilityOutcome::Applied,
            "Something used Ordinary attack on you.",
        ),
        (
            Some(super::actor_target(2)),
            Some(before.self_target),
            "custom.strike",
            "Ordinary attack",
            AbilityOutcome::Applied,
            "The ruin scout used Ordinary attack on you.",
        ),
        (
            Some(before.self_target),
            Some(super::actor_target(2)),
            "unknown.technique",
            "Basic Melee",
            AbilityOutcome::Applied,
            "You used Basic Melee on the ruin scout.",
        ),
        (
            Some(before.self_target),
            Some(super::actor_target(2)),
            "custom.strike",
            "Ordinary attack",
            AbilityOutcome::Unaffected,
            "You used Ordinary attack on the ruin scout.",
        ),
    ] {
        after.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
            caster,
            target,
            ability: ContentRef {
                id: id.into(),
                name: name.into(),
            },
            outcome,
            strike: None,
        }];
        let lines = tor_client_common::narration::observation_lines(&before, &after, None);
        let mut messages = Messages::default();
        messages.absorb(&lines, &after);
        assert_eq!(messages.text(), expected);
    }
    for caster in [None, Some(super::actor_target(2))] {
        after.combat.as_mut().unwrap().events = vec![CombatEventView::UnseenAbility {
            caster,
            target: after.self_target,
            outcome: AbilityOutcome::Applied,
        }];
        let lines = tor_client_common::narration::observation_lines(&before, &after, None);
        let mut messages = Messages::default();
        messages.absorb(&lines, &after);
        assert_eq!(messages.text(), "Something attacks you.");
    }
}

#[test]
fn concise_combat_keeps_special_effects_interruptions_and_actionable_feedback() {
    let before = basic_attack_view();
    let mut after = before.clone();
    after.tick += 1;
    after.combat.as_mut().unwrap().events = vec![
        CombatEventView::Ability {
            caster: Some(after.self_target),
            target: Some(super::actor_target(2)),
            ability: tor_client_common::test_support::content("fear"),
            outcome: AbilityOutcome::Applied,
            strike: None,
        },
        CombatEventView::Condition {
            condition: tor_client_common::test_support::content("poisoned"),
        },
        CombatEventView::Interrupted {
            actor: after.self_target,
        },
        CombatEventView::Died {
            actor: after.self_target,
        },
    ];
    let start = Event::AbilityStarted {
        ability: tor_client_common::test_support::content("fear"),
        target: super::actor_target(2),
    };
    let lines = tor_client_common::narration::observation_lines(&before, &after, Some(&start));
    let mut messages = Messages::default();
    messages.absorb(&lines, &after);
    messages.absorb(
        &[line(
            Topic::Own(Event::PreparationPaused),
            "Your preparation is paused until you act again.",
        )],
        &after,
    );
    messages.push("The target is out of reach.");
    assert_eq!(
        messages.text(),
        "You prepare to use Fear.  You used Fear on the ruin scout.  You are affected by Poisoned.  Your attack was interrupted.  You died.  Your preparation is paused until you act again.  The target is out of reach."
    );
}

#[test]
fn routine_lines_are_left_out_and_kills_read_as_one_sentence() {
    let me = super::actor_target(1);
    let scout = super::actor_target(2);
    let hit = CombatEventView::Ability {
        strike: None,
        ability: tor_client_common::test_support::content("basic_melee"),
        caster: Some(me),
        target: Some(scout),
        outcome: AbilityOutcome::Applied,
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
    assert_eq!(
        messages.text(),
        "You struck the ruin scout.  The ruin scout died."
    );
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
    let miss = CombatEventView::Ability {
        strike: None,
        ability: tor_client_common::test_support::content("basic_melee"),
        caster: Some(super::actor_target(2)),
        target: Some(super::actor_target(1)),
        outcome: AbilityOutcome::Miss,
    };
    for _ in 0..3 {
        messages.absorb(
            &[line(
                Topic::Combat(miss.clone()),
                "The ruin scout missed you.",
            )],
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
        o.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
            strike: None,
            ability: tor_client_common::test_support::content("basic_melee"),
            caster: Some(super::actor_target(2)),
            target: Some(super::actor_target(1)),
            outcome: AbilityOutcome::Applied,
        }]
    });
    // Later updates with nothing to say don't clear what was said.
    update(&mut app, 2, |o| o.combat.as_mut().unwrap().events.clear());
    assert_eq!(
        app.messages.text(),
        "The ruin scout used Basic Melee on you."
    );
    assert!(matches!(
        app.input(Input::Key { key: Key::Up }),
        Effect::Request(_)
    ));
    assert_eq!(app.messages.text(), "");
    assert_eq!(
        app.messages.log().collect::<Vec<_>>(),
        ["The ruin scout used Basic Melee on you."]
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
            "trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,"lit":true,"place_hint":false
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
    let tiles = render::map_tiles(&state, &Default::default());
    let at = |x: i32, y: i32| {
        tiles
            .iter()
            .find(|t| (t.position.x, t.position.y) == (x, y))
            .unwrap()
    };
    assert_eq!((at(0, 0).glyph, at(0, 0).kind), ('@', map::Kind::Player));
    assert_eq!(tiles.iter().filter(|t| t.glyph == '@').count(), 1);
    assert!(!tiles.iter().any(|t| t.position.z == -17));
    assert_eq!((at(2, 0).glyph, at(2, 0).kind), ('&', map::Kind::Creature));
    assert_eq!(
        at(2, 0).color,
        map::creature(&Default::default(), None).color
    );
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
        super::request(app.input(Input::Text { text: "b".into() })),
        Some(Request::Command {
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
    assert_eq!(app.messages.text(), "& - a ruin scout (hostile, wounded).");
    assert_eq!(app.input(Input::Key { key: Key::Travel }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Descend }), Effect::None);
    assert_eq!(app.travel_cursor.map(|c| (c.x, c.y)), Some((-3, 0)));
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command {
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
fn automatic_run_waits_for_character_readiness_after_acknowledgement_and_execution() {
    fn apply(app: &mut App, body: UpdateBody) {
        let state = app.state.as_ref().unwrap();
        app.update(StreamUpdate {
            context: state.context().clone(),
            actor: state.state().observation.actor,
            branch: state.branch().clone(),
            cursor: StreamCursor {
                sequence: state.cursor().sequence + 1,
                tick: state.state().observation.tick,
            },
            body,
        })
        .unwrap();
    }

    for ready_before_permissions in [false, true] {
        let mut initial = snapshot();
        let view = &mut Arc::make_mut(&mut initial.state).observation;
        view.visible_actors
            .retain(|actor| actor.id == view.self_target);
        view.ground_items.clear();
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(initial).unwrap());
        app.ready();
        assert!(matches!(
            app.input(Input::Key { key: Key::RunRight }),
            Effect::Request(_)
        ));
        assert_eq!(app.continue_run(), Effect::None, "request is in flight");

        let status = IntentionStatus {
            actor: ActorId(1),
            branch: BranchId("turns".into()),
            intention: IntentionId("run-step".into()),
            entry_id: EntryId("run-step-entry".into()),
            phase: IntentionPhase::Queued,
        };
        apply(
            &mut app,
            UpdateBody::Intention {
                status: status.clone(),
            },
        );
        apply(
            &mut app,
            UpdateBody::Readiness {
                readiness: Readiness {
                    revision: 1,
                    admission: false,
                    resume: vec![],
                    cancel: vec![status.intention.clone()],
                },
            },
        );
        // The request acknowledgement releases transport busy, not gameplay.
        app.ready();
        assert_eq!(app.continue_run(), Effect::None, "move is still queued");
        update(&mut app, 3, |view| view.ready = ready_before_permissions);
        assert_eq!(
            app.continue_run(),
            Effect::None,
            "execution is not yet disclosed"
        );
        apply(
            &mut app,
            UpdateBody::Intention {
                status: IntentionStatus {
                    phase: IntentionPhase::Resolved,
                    ..status
                },
            },
        );
        assert_eq!(
            app.continue_run(),
            Effect::None,
            "permissions must follow execution"
        );
        apply(
            &mut app,
            UpdateBody::Readiness {
                readiness: Readiness {
                    revision: 2,
                    admission: true,
                    resume: vec![],
                    cancel: vec![],
                },
            },
        );
        if !ready_before_permissions {
            // An empty intention slot permits explicit queued input while other
            // actors still advance. Automatic running must await their results.
            assert!(app.state.as_ref().unwrap().can_admit_intention());
            assert_eq!(
                app.continue_run(),
                Effect::None,
                "admission is not character readiness"
            );
            assert_eq!(app.running, Some(Direction::East));
            update(&mut app, 6, |view| view.ready = true);
        }
        let expected_revision = app.state.as_ref().unwrap().state().revision;
        assert!(matches!(
            super::request(app.continue_run()),
            Some(Request::Command {
                context: InputContext { readiness_revision: 2, .. },
                command: Command::Act {
                    expected_revision: actual_revision,
                    action: Action::Move { direction: Direction::East },
                },
                ..
            }) if actual_revision == expected_revision
        ));
        assert_eq!(
            app.continue_run(),
            Effect::None,
            "only one next step is submitted"
        );
    }
}

#[test]
fn an_explicit_step_can_still_use_an_available_queue_slot_before_character_readiness() {
    let mut initial = snapshot();
    let view = &mut Arc::make_mut(&mut initial.state).observation;
    view.visible_actors
        .retain(|actor| actor.id == view.self_target);
    view.ground_items.clear();
    view.ready = false;
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(initial).unwrap());
    app.ready();
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Right })),
        Some(Request::Command {
            command: Command::Act {
                action: Action::Move {
                    direction: Direction::East
                },
                ..
            },
            ..
        })
    ));
}

#[test]
fn running_ignores_own_body_occurrences_but_keeps_current_creature_stops() {
    for hostile in [false, true] {
        let mut app = app();
        update(&mut app, 1, |o| {
            o.visible_actors.retain(|actor| actor.id == o.self_target);
            o.ground_items.clear();
            // A second view of our own body is still us, even in a different
            // map column. The fixture already includes our head at z = 1.
            let mut own = o.visible_actors[0].clone();
            own.position = Position { x: 2, y: 0, z: 1 };
            o.visible_actors.push(own);
        });
        assert!(!app.creature_in_view());
        assert!(matches!(
            app.input(Input::Key { key: Key::RunRight }),
            Effect::Request(_)
        ));
        app.ready();
        assert!(matches!(app.continue_run(), Effect::Request(_)));
        app.ready();
        update(&mut app, 2, |o| {
            let mut creature = o.visible_actors[0].clone();
            creature.id = super::actor_target(2);
            creature.name = "nearby creature".into();
            creature.position = Position { x: 2, y: 0, z: 1 };
            o.visible_actors.push(creature);
            o.combat.as_mut().unwrap().actors[0].hostile = hostile;
        });
        // Clear the discovery message to prove the creature itself is a stop.
        app.messages.begin_turn();
        assert_eq!(app.continue_run(), Effect::None);
        assert_eq!(app.running, None);
        assert_eq!(app.input(Input::Key { key: Key::RunRight }), Effect::None);
        assert_eq!(app.status, "You can't run with a creature in view.");
        update(&mut app, 3, |o| {
            o.visible_actors.retain(|actor| actor.id == o.self_target);
        });
        // A remembered sighting does not keep blocking a later run.
        assert!(!app.creature_in_view());
        assert!(matches!(
            app.input(Input::Key { key: Key::RunRight }),
            Effect::Request(_)
        ));
    }
}

#[test]
fn running_stops_for_messages_and_a_fresh_key_without_an_extra_action() {
    for stop_with_key in [false, true] {
        let mut app = app();
        update(&mut app, 1, |o| {
            o.visible_actors.retain(|actor| actor.id == o.self_target);
            o.ground_items.clear();
        });
        assert!(matches!(
            app.input(Input::Key { key: Key::RunRight }),
            Effect::Request(_)
        ));
        app.ready();
        let before = app.state.as_ref().unwrap().snapshot();
        if stop_with_key {
            assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
        } else {
            app.messages.push("Something happens.");
            assert_eq!(app.continue_run(), Effect::None);
        }
        assert_eq!(app.running, None);
        assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    }
}

#[test]
fn horizontal_bumps_select_disclosed_flying_body_occurrences() {
    for (key, x, y) in [
        (Key::Up, 0, -1),
        (Key::Down, 0, 1),
        (Key::Left, -1, 0),
        (Key::Right, 1, 0),
        (Key::NorthWest, -1, -1),
        (Key::NorthEast, 1, -1),
        (Key::SouthWest, -1, 1),
        (Key::SouthEast, 1, 1),
    ] {
        for z in [1, 2] {
            let mut snapshot = snapshot();
            let view = &mut Arc::make_mut(&mut snapshot.state).observation;
            view.ground_items.clear();
            for cell in &mut view.visible_cells {
                if cell.position.z == 2 {
                    cell.wall = false;
                }
            }
            view.visible_actors[1].position = Position { x, y, z };
            view.combat.as_mut().unwrap().own_stats =
                Some(tor_client_common::test_support::stats());
            let target = view.visible_actors[1].id;
            let mut app = App::new();
            app.role = AccessRole::Player;
            app.set_state(ClientState::from_snapshot(snapshot).unwrap());
            app.ready();
            let Some(Request::Command {
                command: Command::Act { action, .. },
                ..
            }) = super::request(app.input(Input::Key { key }))
            else {
                panic!("expected one bump command")
            };
            assert!(
                matches!(action, Action::UseAbility { ref id, ref invocation, .. }
                if id == "basic_melee"
                    && invocation.aim == (AbilityAim::ActorTarget { target })),
                "{key:?} must select the adjacent flying creature at z = {z}: {action:?}"
            );
        }
    }
}

fn bump_app(change: impl FnOnce(&mut Observation)) -> App {
    let mut snapshot = snapshot();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    view.ground_items.clear();
    view.visible_actors
        .retain(|actor| actor.id == view.self_target);
    for cell in &mut view.visible_cells {
        if cell.position.z == 2 {
            cell.wall = false;
        }
    }
    let combat = view.combat.as_mut().unwrap();
    combat.actors.clear();
    combat.own_stats = Some(tor_client_common::test_support::stats());
    change(view);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["creature.bird.kestrel".into(), "creature.griffin".into()].into(),
        },
    });
    app.ready();
    app
}

fn bump_actor(view: &mut Observation, id: u64, z: i32, hostile: bool) {
    let kestrel = id == 2;
    view.visible_actors.push(ActorView {
        appearance_seed: None,
        asset: Some(
            if kestrel {
                "creature.bird.kestrel"
            } else {
                "creature.griffin"
            }
            .into(),
        ),
        id: super::actor_target(id),
        name: if kestrel { "kestrel" } else { "griffin" }.into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z },
    });
    if !view
        .combat
        .as_ref()
        .unwrap()
        .actors
        .iter()
        .any(|actor| actor.actor == super::actor_target(id))
    {
        view.combat.as_mut().unwrap().actors.push(CombatActorView {
            actor: super::actor_target(id),
            hostile,
            allied: false,
            injury: Injury::Healthy,
        });
    }
}

fn bump_action(app: &mut App, key: Key) -> Action {
    let Some(Request::Command {
        command: Command::Act { action, .. },
        ..
    }) = super::request(app.input(Input::Key { key }))
    else {
        panic!("expected a gameplay action")
    };
    action
}

fn assert_bump_target(action: Action, target: u64) {
    assert!(
        matches!(action, Action::UseAbility { ref id, ref invocation, .. }
        if id == "basic_melee"
            && invocation.aim == (AbilityAim::ActorTarget { target: super::actor_target(target) })),
        "expected basic melee against {target}, got {action:?}"
    );
}

#[test]
fn bump_map_and_look_choose_the_nearby_actor_over_a_high_column_occupant() {
    for low_height in [0, 1, 2] {
        for high_first in [false, true] {
            let mut app = bump_app(|view| {
                if high_first {
                    bump_actor(view, 3, 3, true);
                }
                bump_actor(view, 2, low_height, true);
                if !high_first {
                    bump_actor(view, 3, 3, true);
                }
            });
            let state = app.state.as_ref().unwrap();
            let tile = render::map_tiles(state, &app.palette)
                .into_iter()
                .find(|tile| (tile.position.x, tile.position.y) == (1, 0))
                .unwrap();
            assert_eq!(tile.glyph, 'K');
            assert!(
                tor_client_ascii::describe(state, &app.palette, 1, 0).starts_with("K - a kestrel")
            );
            assert_bump_target(bump_action(&mut app, Key::Right), 2);
        }
    }
}

#[test]
fn bump_column_priority_prefers_the_exact_destination_and_deduplicates_a_body() {
    let mut app = bump_app(|view| {
        bump_actor(view, 3, 1, true);
        bump_actor(view, 3, 2, true);
        bump_actor(view, 2, 0, true);
    });
    assert_bump_target(bump_action(&mut app, Key::Right), 2);
    let mut app = bump_app(|view| {
        bump_actor(view, 2, 2, true);
        bump_actor(view, 2, 1, true);
    });
    assert_bump_target(bump_action(&mut app, Key::Right), 2);
    assert_eq!(
        app.input(Input::Key { key: Key::Right }),
        Effect::None,
        "one body produces one request while it is outstanding"
    );
}

#[test]
fn bump_height_envelope_uses_only_the_origin_connected_disclosed_body() {
    for connected in [false, true] {
        let mut app = bump_app(|view| {
            let mut own = view.visible_actors[0].clone();
            own.position = if connected {
                Position { x: 0, y: 0, z: 2 }
            } else {
                Position { x: 6, y: 0, z: 2 }
            };
            view.visible_actors.push(own);
            bump_actor(view, 2, 3, true);
        });
        let action = bump_action(&mut app, Key::Right);
        if connected {
            assert_bump_target(action, 2);
        } else {
            assert_eq!(
                action,
                Action::Move {
                    direction: Direction::East
                },
                "a high flyer and a remote self occurrence do not block a legal step"
            );
        }
    }
}

#[test]
fn elevated_bumps_preserve_modes_disclosure_and_server_refusal() {
    use tor_client_ascii::BumpAttacks;
    for (mode, hostile, attack) in [
        (BumpAttacks::Hostile, true, true),
        (BumpAttacks::Hostile, false, false),
        (BumpAttacks::Any, false, true),
        (BumpAttacks::Off, true, false),
    ] {
        let mut app = bump_app(|view| bump_actor(view, 2, 1, hostile));
        app.bump_attacks = mode;
        let action = bump_action(&mut app, Key::Right);
        if attack {
            assert_bump_target(action, 2);
            app.ready();
            app.refused("You can't do that now.".into());
            assert_eq!(app.status, "You can't do that now.");
            assert_eq!(
                app.continue_run(),
                Effect::None,
                "refusal does not retry or silently substitute a move"
            );
        } else {
            assert_eq!(
                action,
                Action::Move {
                    direction: Direction::East
                }
            );
        }
    }
    let mut app = bump_app(|view| {
        bump_actor(view, 2, 1, true);
        view.visible_actors
            .retain(|actor| actor.id == view.self_target);
    });
    assert_eq!(
        bump_action(&mut app, Key::Right),
        Action::Move {
            direction: Direction::East
        },
        "a combat relationship alone never supplies a hidden target"
    );
    let mut app = bump_app(|view| bump_actor(view, 2, 4, true));
    assert_eq!(
        bump_action(&mut app, Key::Right),
        Action::Move {
            direction: Direction::East
        },
        "an occurrence outside the rendered height band is not a bump target"
    );
}

#[test]
fn hostile_bumps_do_not_skip_a_rendered_neutral_actor_for_another_column_occupant() {
    for hostile_first in [false, true] {
        let mut app = bump_app(|view| {
            if hostile_first {
                bump_actor(view, 3, 1, true);
            }
            bump_actor(view, 2, 0, false);
            if !hostile_first {
                bump_actor(view, 3, 1, true);
            }
        });
        let state = app.state.as_ref().unwrap();
        assert!(tor_client_ascii::describe(state, &app.palette, 1, 0)
            .starts_with("K - a kestrel (peaceful"));
        assert_eq!(
            bump_action(&mut app, Key::Right),
            Action::Move {
                direction: Direction::East
            }
        );
        app.ready();
        app.bump_attacks = tor_client_ascii::BumpAttacks::Any;
        assert_bump_target(bump_action(&mut app, Key::Right), 2);
    }
}

#[test]
fn a_connected_horizontal_self_body_extension_can_supply_a_local_bump_candidate() {
    for connected in [false, true] {
        let mut app = bump_app(|view| {
            let own = view.visible_actors[0].clone();
            if connected {
                view.visible_actors.push(ActorView {
                    position: Position { x: 1, y: 0, z: 1 },
                    ..own.clone()
                });
            }
            view.visible_actors.push(ActorView {
                position: Position { x: 1, y: 0, z: 2 },
                ..own
            });
            bump_actor(view, 2, 3, true);
        });
        let action = bump_action(&mut app, Key::Right);
        if connected {
            assert_bump_target(action, 2);
        } else {
            assert_eq!(
                action,
                Action::Move {
                    direction: Direction::East
                }
            );
        }
    }
}

#[test]
fn vertical_bumps_keep_exact_occurrence_selection() {
    for (key, z) in [(Key::Ascend, 1), (Key::Descend, -1)] {
        let mut app = bump_app(|view| {
            view.visible_actors.clear();
            bump_actor(view, 2, z, true);
            view.visible_actors[0].position.x = 0;
            bump_actor(view, 3, z, true);
        });
        assert_bump_target(bump_action(&mut app, key), 2);
    }
}

#[test]
fn bump_columns_use_the_observation_origin_and_checked_neighbor_coordinates() {
    let mut app = bump_app(|view| {
        bump_actor(view, 2, 1, true);
        let translate = |position: &mut Position| {
            position.x += 12;
            position.y -= 9;
            position.z += 4;
        };
        translate(&mut view.position);
        for cell in &mut view.visible_cells {
            translate(&mut cell.position);
        }
        for actor in &mut view.visible_actors {
            translate(&mut actor.position);
        }
    });
    let state = app.state.as_ref().unwrap();
    assert!(tor_client_ascii::describe(state, &app.palette, 13, -9).starts_with("K - a kestrel"));
    assert_bump_target(bump_action(&mut app, Key::Right), 2);

    let mut app = bump_app(|view| {
        view.position.x = i32::MAX;
        view.visible_actors.clear();
    });
    assert_eq!(bump_action(&mut app, Key::Right), Action::Move { direction: Direction::East },
        "an overflowing client coordinate does not fabricate a target or replace server movement authority");
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
    assert_eq!(second, "HP:40(50)  Gold:0  T:100");
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

#[test]
fn map_look_and_inventory_share_assets_without_renaming_disclosed_things() {
    let mut snapshot = snapshot();
    let o = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    o.visible_actors[1].asset = Some("creature.equine.quagga.unfamiliar".into());
    // A deliberate name/class mismatch proves presentation does not infer an
    // identity from either the name or a hidden item archetype.
    o.ground_items[0].item.asset = Some("item.amulet".into());
    o.inventory[0].asset = Some("item.amulet".into());
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: [
                "creature.equine.quagga.unfamiliar".into(),
                "item.amulet".into(),
            ]
            .into(),
        },
    });
    let state = app.state.as_ref().unwrap();
    let unchanged = state.snapshot();
    let tiles = render::map_tiles(state, &app.palette);
    let actor = tiles
        .iter()
        .find(|tile| tile.kind == map::Kind::Creature)
        .unwrap();
    assert_eq!(actor.glyph, 'u');
    assert_eq!(
        tor_client_ascii::describe(state, &app.palette, 2, 0),
        "u - a ruin scout (hostile, wounded)."
    );
    let item = tiles
        .iter()
        .find(|tile| tile.kind == map::Kind::Item)
        .unwrap();
    let carried = tor_client_ascii::inventory_entries(&app, &state.state().observation);
    assert_eq!(
        (item.glyph, item.color),
        (carried[0].glyph, carried[0].color)
    );
    assert_eq!(carried[0].glyph, '"');
    assert_eq!(carried[0].name, "red potion");
    assert_eq!(
        tor_client_ascii::describe(state, &app.palette, -1, 0),
        "\" - a sword."
    );
    assert_eq!(state.snapshot(), unchanged);

    // A palette-only loss and recovery changes appearance without changing
    // observation, inventory handles, commands, memory or authored names.
    app.palette.apply(&PaletteUpdate {
        revision: 3,
        body: PaletteBody::Delta {
            base: 2,
            added: Default::default(),
            removed: Default::default(),
        },
    });
    assert_eq!(
        render::map_tiles(state, &app.palette)
            .iter()
            .find(|tile| tile.kind == map::Kind::Creature)
            .unwrap()
            .glyph,
        '&'
    );
    assert_eq!(
        tor_client_ascii::inventory_entries(&app, &state.state().observation)[0].glyph,
        '!'
    );
    app.palette.apply(&PaletteUpdate {
        revision: 4,
        body: PaletteBody::Full {
            assets: [
                "creature.equine.quagga.unfamiliar".into(),
                "item.amulet".into(),
            ]
            .into(),
        },
    });
    assert_eq!(render::map_tiles(state, &app.palette), tiles);
    assert_eq!(state.snapshot(), unchanged);
}

#[test]
fn unknown_missing_and_unforecast_assets_have_name_independent_safe_fallbacks() {
    let mut snapshot = snapshot();
    let o = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    o.visible_actors[1].asset = Some("unknown.actor".into());
    let mut palette = tor_client_common::Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["unknown.actor".into()].into(),
        },
    });
    let before = ClientState::from_snapshot(snapshot.clone()).unwrap();
    let o = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    o.visible_actors[1].name = "an entirely different proper name".into();
    let after = ClientState::from_snapshot(snapshot).unwrap();
    assert_eq!(
        render::map_tiles(&before, &palette),
        render::map_tiles(&after, &palette)
    );
    for asset in [None, Some("unknown.actor"), Some("creature.equine.quagga")] {
        assert_eq!(map::creature(&palette, asset).glyph, '&');
    }
}

#[test]
fn unnamed_actor_look_uses_the_shared_english_indefinite_form() {
    for (asset, phrase) in [
        ("creature.equine.unicorn", "u - a unicorn"),
        ("creature.bird.emu", "B - an emu"),
    ] {
        let mut snapshot = snapshot();
        let actor = &mut Arc::make_mut(&mut snapshot.state)
            .observation
            .visible_actors[1];
        actor.name.clear();
        actor.asset = Some(asset.into());
        let state = ClientState::from_snapshot(snapshot).unwrap();
        let mut palette = tor_client_common::Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: [asset.into()].into(),
            },
        });
        assert_eq!(
            tor_client_ascii::describe(&state, &palette, 2, 0),
            format!("{phrase} (hostile, wounded).")
        );
    }
}

#[test]
fn cosmetic_items_and_unknown_stairs_change_art_not_map_authority() {
    let mut o = view();
    let real = o.ground_items[0].item.clone();
    o.ground_items[0].item.name = "object".into();
    o.ground_items[0].item.appearance = "object".into();
    o.ground_items[0].item.description.clear();
    o.ground_items[0].item.asset = None;
    o.ground_items[0].presentation = Some(ObjectPresentation {
        name: "stairs".into(),
        asset: None,
        kind: ObjectPresentationKind::Stairs,
    });
    let palette = tor_client_common::Palette::default();
    let tiles = map::tiles(&o, &palette, &|_| true);
    let fake = tiles
        .iter()
        .find(|tile| tile.position.x == -1 && tile.position.y == 0)
        .unwrap();
    assert_eq!(fake.glyph, '>');
    assert_eq!(fake.kind, map::Kind::Item);
    assert_eq!(o.ground_items[0].item.id, real.id);
    assert_eq!(o.ground_items[0].item.class, real.class);
    assert_eq!(o.ground_items[0].item.quantity, real.quantity);
    let known = tiles
        .iter()
        .find(|tile| tile.position.x == -3 && tile.position.y == 0)
        .unwrap();
    assert_eq!(known.glyph, '>');
    assert_eq!(known.kind, map::Kind::StairsDown);
    let stairs = o
        .visible_cells
        .iter_mut()
        .find(|cell| cell.stairs_down)
        .unwrap();
    stairs.object_presentation = Some(ObjectPresentation {
        name: "silver flask".into(),
        asset: None,
        kind: ObjectPresentationKind::Item {
            class: ItemClass::Potion,
        },
    });
    let tiles = map::tiles(&o, &palette, &|_| true);
    let altered = tiles
        .iter()
        .find(|tile| tile.position.x == -3 && tile.position.y == 0)
        .unwrap();
    assert_eq!(altered.glyph, '!');
    assert_eq!(altered.kind, map::Kind::StairsDown);
    assert!(o.visible_cells.iter().any(|cell| cell.stairs_down));
}

#[test]
fn cosmetic_look_picker_and_take_use_display_name_but_original_opaque_item() {
    let mut snap = snapshot();
    let o = &mut Arc::make_mut(&mut snap.state).observation;
    o.ground_items[0].item.name = "object".into();
    o.ground_items[0].item.appearance = "object".into();
    o.ground_items[0].item.description.clear();
    o.ground_items[0].item.asset = None;
    o.ground_items[0].presentation = Some(ObjectPresentation {
        name: "silver flask".into(),
        asset: None,
        kind: ObjectPresentationKind::Item {
            class: ItemClass::Potion,
        },
    });
    let mut other = o.ground_items[0].clone();
    other.item.id = super::item_target(88);
    other.presentation.as_mut().unwrap().name = "stairs".into();
    other.presentation.as_mut().unwrap().kind = ObjectPresentationKind::Stairs;
    o.ground_items.push(other);
    let state = ClientState::from_snapshot(snap).unwrap();
    let look = tor_client_ascii::describe(&state, &tor_client_common::Palette::default(), -1, 0);
    assert!(look.contains("silver flask"), "{look}");
    assert!(!look.contains("sword"), "{look}");
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Pickup }), Effect::None);
    let rows = app.item_rows().join("\n");
    assert!(
        rows.contains("silver flask") && rows.contains("stairs"),
        "{rows}"
    );
    assert!(!rows.contains("sword"), "{rows}");
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command { command: Command::Act { action: Action::Take { item, .. }, .. }, .. })
        if item == super::item_target(7))
    );
}
