mod capture;
mod native_report;
mod network;

use minifb::{InputCallback, Key as NativeKey, KeyRepeat, ScaleMode, Window, WindowOptions};
use network::{Command, Event, Network};
use std::{
    cell::RefCell,
    io::{self, BufRead, Write},
    net::SocketAddr,
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::{Duration, Instant},
};
use tor_client_ascii::{
    render::{Canvas, HEIGHT, WIDTH},
    App, Effect, Input, Key,
};
use tor_protocol::ActorId;

type Error = Box<dyn std::error::Error + Send + Sync>;
struct TextInput(Rc<RefCell<String>>);
impl InputCallback for TextInput {
    fn add_char(&mut self, value: u32) {
        if let Some(ch) = char::from_u32(value).filter(|c| !c.is_control()) {
            let mut buffer = self.0.borrow_mut();
            if buffer.len() + ch.len_utf8() <= 4096 {
                buffer.push(ch);
            }
        }
    }
}

/// Supported non-numpad defaults from NetHack's official Commands section:
/// https://nethack.org/v500/Guidebook.html (also present in the 3.6.7 guide).
/// TOR-only commands live in `#`; unsupported NetHack letters never fall back
/// to an unrelated lowercase action.
fn native_key_with_modifiers(key: NativeKey, shift: bool, ctrl: bool) -> Option<Key> {
    if ctrl {
        return match key {
            NativeKey::P => Some(Key::MessageLog),
            NativeKey::X => Some(Key::Stats),
            NativeKey::O => Some(Key::Places),
            _ => None,
        };
    }
    if shift {
        return match key {
            NativeKey::Comma => Some(Key::Ascend),
            NativeKey::Period => Some(Key::Descend),
            NativeKey::Slash => Some(Key::Help),
            NativeKey::Key3 => Some(Key::Commands),
            NativeKey::Up | NativeKey::K => Some(Key::RunUp),
            NativeKey::Down | NativeKey::J => Some(Key::RunDown),
            NativeKey::Left | NativeKey::H => Some(Key::RunLeft),
            NativeKey::Right | NativeKey::L => Some(Key::RunRight),
            NativeKey::Y => Some(Key::RunNorthWest),
            NativeKey::U => Some(Key::RunNorthEast),
            NativeKey::B => Some(Key::RunSouthWest),
            NativeKey::N => Some(Key::RunSouthEast),
            NativeKey::G => Some(Key::RunPrefix),
            NativeKey::P => Some(Key::PutOn),
            NativeKey::R => Some(Key::RemoveAccessory),
            NativeKey::S => Some(Key::SaveQuit),
            NativeKey::T => Some(Key::TakeOff),
            NativeKey::W => Some(Key::Wear),
            NativeKey::Z => Some(Key::Abilities),
            // Modifiers do not change these non-character controls.
            NativeKey::Space
            | NativeKey::Enter
            | NativeKey::Escape
            | NativeKey::Backspace
            | NativeKey::Tab
            | NativeKey::PageUp
            | NativeKey::PageDown
            | NativeKey::F2
            | NativeKey::F3
            | NativeKey::F4
            | NativeKey::F5
            | NativeKey::F6
            | NativeKey::F7
            | NativeKey::F8
            | NativeKey::F9
            | NativeKey::F10
            | NativeKey::F11
            | NativeKey::F12 => native_key(key),
            _ => None,
        };
    }
    native_key(key)
}

/// Brackets retain their existing private-report pagination meaning, not a
/// global pace shortcut. Unsupported Meta/Alt commands must not act as letters.
fn native_input_key(
    key: NativeKey,
    shift: bool,
    ctrl: bool,
    alt: bool,
    strike_detail: bool,
) -> Option<Key> {
    if alt {
        return None;
    }
    if strike_detail && !shift && !ctrl {
        match key {
            NativeKey::LeftBracket => return Some(Key::Slower),
            NativeKey::RightBracket => return Some(Key::Faster),
            _ => {}
        }
    }
    native_key_with_modifiers(key, shift, ctrl)
}

fn is_letter(key: NativeKey) -> bool {
    use NativeKey::*;
    matches!(
        key,
        A | B
            | C
            | D
            | E
            | F
            | G
            | H
            | I
            | J
            | K
            | L
            | M
            | N
            | O
            | P
            | Q
            | R
            | S
            | T
            | U
            | V
            | W
            | X
            | Y
            | Z
    )
}

/// Typed command characters arrive through TextInput. Their gameplay aliases
/// (especially j/k navigation and shifted #) must not also edit the menu.
fn command_editor_key(key: NativeKey) -> bool {
    matches!(
        key,
        NativeKey::Up
            | NativeKey::Down
            | NativeKey::Enter
            | NativeKey::Escape
            | NativeKey::Backspace
    )
}

fn native_key(key: NativeKey) -> Option<Key> {
    Some(match key {
        NativeKey::A => Key::Apply,
        NativeKey::F => Key::Fire,
        NativeKey::G => Key::RunPrefix,
        NativeKey::M => Key::MoveOnlyPrefix,
        NativeKey::R => Key::Read,
        NativeKey::S => Key::Search,
        NativeKey::T => Key::Throw,
        NativeKey::W => Key::Wield,
        NativeKey::Z => Key::Zap,
        NativeKey::Up | NativeKey::K => Key::Up,
        NativeKey::Down | NativeKey::J => Key::Down,
        NativeKey::Left | NativeKey::H => Key::Left,
        NativeKey::Right | NativeKey::L => Key::Right,
        NativeKey::Y => Key::NorthWest,
        NativeKey::U => Key::NorthEast,
        NativeKey::B => Key::SouthWest,
        NativeKey::N => Key::SouthEast,
        NativeKey::Space | NativeKey::Period => Key::Wait,
        NativeKey::Comma => Key::Pickup,
        NativeKey::I => Key::Inventory,
        NativeKey::Semicolon | NativeKey::Slash => Key::Look,
        NativeKey::D => Key::Drop,
        NativeKey::Q => Key::Drink,
        NativeKey::E => Key::Eat,
        NativeKey::F6 => Key::Read,
        NativeKey::F12 => Key::Zap,
        NativeKey::O => Key::OpenDoor,
        NativeKey::C => Key::CloseDoor,
        NativeKey::F3 => Key::Control,
        NativeKey::F8 => Key::ResumeIntention,
        NativeKey::F9 => Key::CancelIntention,
        NativeKey::F10 => Key::Advancement,
        NativeKey::F11 => Key::EscapeHold,
        NativeKey::F4 => Key::Note,
        NativeKey::F7 => Key::Wizard,
        NativeKey::F5 => Key::Places,
        NativeKey::Enter => Key::Enter,
        NativeKey::Escape => Key::Escape,
        NativeKey::Backspace => Key::Backspace,
        NativeKey::Tab => Key::Tab,
        NativeKey::F2 => Key::History,
        NativeKey::PageUp => Key::OlderHistory,
        NativeKey::PageDown => Key::RecentHistory,
        _ => return None,
    })
}

fn main() {
    if let Err(error) = run() {
        eprintln!(
            "ASCII client: {}",
            error
                .to_string()
                .chars()
                .flat_map(char::escape_default)
                .collect::<String>()
        );
        std::process::exit(1);
    }
}

fn run() -> Result<(), Error> {
    let mut address: SocketAddr = "127.0.0.1:4000".parse()?;
    let mut actor = ActorId(1);
    let mut observe = false;
    let mut bump_attacks = tor_client_ascii::BumpAttacks::Hostile;
    let mut pace_ms = tor_client_ascii::DEFAULT_PACE_MS;
    let mut automation = false;
    let mut report = false;
    let mut capture = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Z: choose a granted ability (Up/Down selects, Enter confirms, Esc cancels); z: zap a carried device.");
                println!(
                    "Ctrl-X: personal creature stats (Up/Down scroll, PgUp/PgDn page, Esc closes)."
                );
                println!("tor-client-ascii [--connect 127.0.0.1:4000] [--actor 1] [--observe] [--pace 75]\nSet TOR_SERVER_TOKEN to the server token. A native graphical display is required.\nNetHack-style non-numpad keys are case-sensitive. # opens TOR's extended commands, including attack, charts, control/release and journey pace.\nArrows/hjklyubn: move (Shift runs); g/G then direction: run with TOR stop rules; m then direction: move without bump attacking; </>: up/down; Space or .: wait; ,: pick up; d: drop; w/W/P: wield/wear/put on; T/R: take off/remove accessory; q: drink; e: eat; r: read; z: zap; a: apply portable light; s: search; t/f: throw/fire if granted.\ni: inventory; ; or /: look; ?: help; Ctrl-P: earlier messages; o/c then direction: open/close adjacent door; _: select travel destination (< > jump to stairs); left click: travel; F3: acquire control; #release: release control. F8/F9: resume/cancel queued work. Move after a completed strike to cancel the sequence and step; outstanding recovery is still owed.\nF7: wizard command editor (wizard accounts); F5/Ctrl-O: remembered places (Up/Down select, Enter rename); F4: note (Tab audience, Enter save, Esc cancel); F2: history (Up/Down scroll, PgUp older, PgDn live); F6/F12: read/zap; F10: advancement; F11: escape a hold.\n#slower/#faster: show journey steps more slowly/quickly (--pace <ms>, default 75); any key during a journey shows the rest at once. [/]: previous/next page only inside strike details.\nEsc: cancel or close, never quit. S: save and close through the normal flush-on-close path. Relaunch to reconnect after a disconnect.\nUnsupported NetHack commands (such as force-fight, weapon swap, two-weapon mode, autopickup and counts) are not implemented.\nProcess tests only: --automation reads JSON input events on stdin and reports presented frames.\n--report-frames reports frames while retaining native keyboard input.\n--capture <file.ppm> with either diagnostic option saves the last presented framebuffer.");
                return Ok(());
            }
            "--connect" => address = args.next().ok_or("Missing --connect address")?.parse()?,
            "--actor" => actor = ActorId(args.next().ok_or("Missing --actor ID")?.parse()?),
            "--observe" => observe = true,
            "--pace" => pace_ms = args.next().ok_or("Missing --pace milliseconds")?.parse()?,
            "--bump-attacks" => {
                bump_attacks = match args.next().as_deref() {
                    Some("hostile") => tor_client_ascii::BumpAttacks::Hostile,
                    Some("any") => tor_client_ascii::BumpAttacks::Any,
                    Some("off") => tor_client_ascii::BumpAttacks::Off,
                    _ => return Err("Use --bump-attacks hostile|any|off".into()),
                }
            }
            "--automation" => {
                automation = true;
                report = true;
            }
            "--report-frames" => report = true,
            "--capture" => {
                capture = Some(PathBuf::from(args.next().ok_or("Missing capture path")?))
            }
            _ => return Err(format!("Unknown argument: {arg}").into()),
        }
    }
    if !address.ip().is_loopback() {
        return Err("Only loopback connections are supported".into());
    }
    if capture.is_some() && !report {
        return Err("--capture requires --automation or --report-frames".into());
    }
    let token = std::env::var("TOR_SERVER_TOKEN")
        .map_err(|_| "Set TOR_SERVER_TOKEN before starting the client")?;
    let mut window = Window::new(
        "Thresholds of Ruin | ASCII",
        WIDTH,
        HEIGHT,
        WindowOptions {
            resize: true,
            scale_mode: ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(60);
    let text = Rc::new(RefCell::new(String::new()));
    window.set_input_callback(Box::new(TextInput(text.clone())));
    let input = automation.then(automation_input);
    let network = Network::start(
        address,
        token,
        actor,
        observe,
        Duration::from_millis(pace_ms),
    );
    let mut app = App::new();
    app.bump_attacks = bump_attacks;
    app.pace_ms = pace_ms;
    let result = window_loop(&mut window, &network, &text, input, report, capture, app);
    network.shutdown()?;
    result
}

fn automation_input() -> mpsc::Receiver<Result<Input, String>> {
    let (tx, rx) = mpsc::sync_channel(16);
    std::thread::spawn(move || {
        for line in io::stdin().lock().lines() {
            let event = line
                .map_err(|e| e.to_string())
                .and_then(|line| serde_json::from_str(&line).map_err(|e| e.to_string()));
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    rx
}

fn window_loop(
    window: &mut Window,
    network: &Network,
    text: &Rc<RefCell<String>>,
    input: Option<mpsc::Receiver<Result<Input, String>>>,
    report: bool,
    capture: Option<PathBuf>,
    mut app: App,
) -> Result<(), Error> {
    let mut canvas = Canvas::default();
    let mut frame = 0u64;
    let mut pending_input = None;
    let mut pending_native_input = None;
    let mut native_reports = native_report::NativeReport::new(report && input.is_none());
    let mut failed = false;
    let mut mouse_down = false;
    let mut previous_report_ms = 0.;
    let mut previous_report_encode_ms = 0.;
    let mut previous_report_write_ms = 0.;
    let mut last_turn = Instant::now();
    while window.is_open() {
        let turn = Instant::now();
        let turn_interval_ms = turn.duration_since(last_turn).as_secs_f64() * 1000.;
        last_turn = turn;
        let mut network_events = 0;
        let mut dirty = frame == 0;
        // Bound each presentation turn so a continuous producer cannot starve input.
        for _ in 0..16 {
            if turn.elapsed() >= std::time::Duration::from_millis(4) {
                break;
            }
            match network.events.try_recv() {
                Ok(event) => {
                    network_events += 1;
                    dirty = true;
                    match event {
                        Event::Role(role) => app.role = role,
                        Event::Palette(palette) => app.palette = palette,
                        Event::Snapshot(snapshot) => app
                            .replace_snapshot(*snapshot)
                            .map_err(|e| format!("Invalid presentation snapshot: {e:?}"))?,
                        Event::Update(update) => app
                            .update(*update)
                            .map_err(|e| format!("Invalid presentation update: {e:?}"))?,
                        Event::MapMemory(update) => {
                            if let Some(state) = app.state.as_mut() {
                                state
                                    .apply_map_memory_update(*update)
                                    .map_err(|e| format!("Invalid presentation map: {e:?}"))?;
                            }
                        }
                        Event::ChartCancelled(query) => app.chart_cancelled(&query),
                        Event::ChartPresentation(update) => {
                            if app.charts_open {
                                if let Some(state) = &app.state {
                                    let _ = app.charts.install(update, state);
                                }
                            }
                        }
                        Event::CreatureInspection(report) => app.show_inspection(&report)?,
                        Event::CreatureCatalog(page) => app.show_creature_catalog(&page)?,
                        Event::ProjectileChoicesSent(id) => {
                            if let Err(error) = app.bind_projectile_request(&id) {
                                app.refused(error);
                            }
                        }
                        Event::ProjectileChoices {
                            context,
                            request_id,
                            page,
                        } => {
                            if let Err(error) =
                                app.show_projectile_choices(&context, &request_id, &page)
                            {
                                app.refused(error);
                            }
                        }
                        Event::WeaponPreviewSent(id) => {
                            if app.weapon_confirmation.is_some() {
                                if let Err(error) = app.bind_weapon_preview(&id) {
                                    app.refused(error);
                                }
                            }
                        }
                        Event::AbilityWeaponPreview {
                            context,
                            request_id,
                            preview,
                        } => {
                            if let Err(error) =
                                app.show_weapon_preview(&context, &request_id, &preview)
                            {
                                app.refused(error);
                            }
                        }
                        Event::StrikeQuerySent(id) => app.bind_strike_request(&id),
                        Event::OverviewPrepared(revision) => {
                            if let Err(error) = app.prepare_overview(revision) {
                                app.refused(error);
                            }
                        }
                        Event::AdvancementQuerySent(id) => {
                            if let Err(error) = app.bind_advancement_request(&id) {
                                app.refused(error);
                            }
                        }
                        Event::AdvancementPage {
                            context,
                            request_id,
                            page,
                        } => {
                            if let Err(error) =
                                app.show_advancement_page(&context, &request_id, &page)
                            {
                                app.refused(error);
                            }
                        }
                        Event::CreatureStrikePage {
                            context,
                            request_id,
                            page,
                        } => {
                            if let Err(error) = app.show_strike_page(&context, &request_id, &page) {
                                app.refused(error);
                            }
                        }
                        Event::CombatDiagnostics(report) => app.show_combat_diagnostics(&report)?,
                        Event::Status(status) => app.status = status,
                        Event::Refused(status) => app.refused(status),
                        Event::Ready => app.ready(),
                        Event::History(page) => {
                            // Open at the newest entries, as the message log does.
                            app.history_scroll = tor_client_ascii::history_lines(&page.entries)
                                .len()
                                .saturating_sub(tor_client_ascii::HISTORY_ROWS);
                            app.history_page = Some(page);
                        }
                        Event::Fatal(error) => {
                            app.disconnect(error);
                            failed = true;
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if !failed {
                        app.disconnect("Connection worker stopped. Relaunch to reconnect.".into());
                        failed = true;
                        dirty = true;
                    }
                    break;
                }
            }
        }
        let apply_ms = turn.elapsed().as_secs_f64() * 1000.;
        let mut inputs = Vec::new();
        let typed = std::mem::take(&mut *text.borrow_mut());
        if input.is_none() {
            // Send text only to an already-open editor.
            if (app.note.is_some()
                || (app.advancement_open && app.supply_selection.is_none())
                || app.wizard_command.is_some()
                || app.command_line.is_some()
                || app.place_name.is_some()
                || !app.pickup.is_empty()
                || app.identification_choice().is_some())
                && !typed.is_empty()
            {
                inputs.push(Input::Text {
                    text: typed.clone(),
                });
            }
            if app.note.is_none()
                && !app.advancement_open
                && app.wizard_command.is_none()
                && app.command_line.is_none()
                && !app.places_open
                && typed.contains('_')
            {
                inputs.push(Input::Key { key: Key::Travel });
            }
            let pressed = window.get_mouse_down(minifb::MouseButton::Left);
            if pressed && !mouse_down {
                if let Some((x, y)) = window.get_unscaled_mouse_pos(minifb::MouseMode::Discard) {
                    let (w, h) = window.get_size();
                    if let Some((x, y)) = tor_client_ascii::render::logical_mouse(x, y, w, h) {
                        inputs.push(Input::Click { x, y });
                    }
                }
            }
            mouse_down = pressed;
            // In a menu, letters choose items (sent as text above) rather
            // than acting as commands.
            let menu = !app.pickup.is_empty() || app.identification_choice().is_some();
            inputs.extend(
                window
                    .get_keys_pressed(KeyRepeat::No)
                    .into_iter()
                    .filter(|key| !(menu && is_letter(*key)))
                    .filter(|key| app.command_line.is_none() || command_editor_key(*key))
                    .filter(|key| {
                        !app.advancement_open
                            || matches!(
                                key,
                                NativeKey::Up
                                    | NativeKey::Down
                                    | NativeKey::PageUp
                                    | NativeKey::PageDown
                                    | NativeKey::Enter
                                    | NativeKey::Escape
                                    | NativeKey::Backspace
                                    | NativeKey::F8
                                    | NativeKey::F9
                                    | NativeKey::F10
                            )
                            || (app.supply_selection.is_some() && *key == NativeKey::Space)
                    })
                    .filter_map(|key| {
                        let mapped = native_input_key(
                            key,
                            window.is_key_down(NativeKey::LeftShift)
                                || window.is_key_down(NativeKey::RightShift),
                            window.is_key_down(NativeKey::LeftCtrl)
                                || window.is_key_down(NativeKey::RightCtrl),
                            window.is_key_down(NativeKey::LeftAlt)
                                || window.is_key_down(NativeKey::RightAlt),
                            app.strike_detail_open(),
                        );
                        if report && mapped.is_some() {
                            pending_native_input = native_reports.pressed(key);
                        }
                        mapped
                    })
                    .map(|key| Input::Key { key }),
            );
        } else if let Some(input) = input.as_ref().filter(|_| !app.busy) {
            match input.try_recv() {
                Ok(event) => {
                    let event = event.map_err(|e| format!("Invalid automation input: {e}"))?;
                    if let Input::Key { key } = &event {
                        pending_input = Some(*key);
                    }
                    inputs.push(event);
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return if failed {
                        Err("Server disconnected".into())
                    } else {
                        Ok(())
                    }
                }
            }
        }
        let mut quit = false;
        let mut effects = Vec::new();
        for event in inputs {
            dirty = true;
            if input.is_none() {
                if let Input::Key { key } = &event {
                    pending_input = Some(*key);
                }
            }
            effects.push(app.input(event));
        }
        // A run takes its next step once the last one is settled.
        if app.running.is_some() {
            let effect = app.continue_run();
            dirty |= effect != Effect::None || app.running.is_none();
            effects.push(effect);
        }
        for effect in effects {
            match effect {
                Effect::CancelCharts => {
                    let _ = network.commands.try_send(Command::CancelCharts);
                }
                Effect::None => {}
                Effect::Quit => {
                    quit = true;
                    break;
                }
                Effect::Skip => {
                    let _ = network.commands.try_send(Command::Skip);
                }
                Effect::Pace(ms) => {
                    let _ = network
                        .commands
                        .try_send(Command::Pace(Duration::from_millis(ms)));
                }
                Effect::OpenOverview(plan) => {
                    if network
                        .commands
                        .try_send(Command::OpenOverview(plan))
                        .is_err()
                    {
                        app.disconnect("Network command queue unavailable.".into());
                        failed = true;
                    }
                }
                Effect::Request(request) => {
                    let context = app
                        .state
                        .as_ref()
                        .ok_or("No attached state for request")?
                        .context()
                        .clone();
                    if network
                        .commands
                        .try_send(Command::Request { context, request })
                        .is_err()
                    {
                        app.disconnect("Network command queue unavailable.".into());
                        failed = true;
                    }
                }
            }
        }
        let mut draw_ms = 0.;
        let native_started;
        if dirty {
            let draw_started = Instant::now();
            canvas.draw(&app);
            draw_ms = draw_started.elapsed().as_secs_f64() * 1000.;
            native_started = Instant::now();
            window.update_with_buffer(&canvas.pixels, WIDTH, HEIGHT)?;
            frame += 1;
        } else {
            // Pump native events without repainting an unchanged 960,000-pixel
            // framebuffer. State/input changes set `dirty` above.
            native_started = Instant::now();
            window.update();
        }
        let native_ms = native_started.elapsed().as_secs_f64() * 1000.;
        // This is intentionally after real native presentation. There is no
        // headless fallback; CI must supply a functioning display environment.
        if report && dirty {
            let report_started = Instant::now();
            let presented_unix_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let done = if !app.busy && app.running.is_none() {
                pending_input.take()
            } else {
                None
            };
            let native_input_sequence = done.and_then(|_| pending_native_input.take());
            let state = app.state.as_ref();
            let capture_started = Instant::now();
            if let Some(path) = &capture {
                capture::save_frame(path, &canvas)?;
            }
            let capture_ms = capture_started.elapsed().as_secs_f64() * 1000.;
            let encode_started = Instant::now();
            let profile = serde_json::json!({
                "version": 1, "network_events": network_events,
                "apply_ms": apply_ms, "draw_ms": draw_ms,
                "previous_report_encode_ms": previous_report_encode_ms,
                "previous_report_write_ms": previous_report_write_ms,
                "native_ms": native_ms, "capture_ms": capture_ms,
                "previous_report_ms": previous_report_ms, "turn_interval_ms": turn_interval_ms
            });
            let mut frame_report = serde_json::json!({"type":"frame","frame":frame,
                "presented_unix_ns":presented_unix_ns,
                "profile":profile,"window_open":window.is_open(),
                "state":state.map(|s|s.state()),"branch":state.map(|s|s.branch()),"history":state.map(|s|s.history()),
                "map_tiles":state.map(|s|tor_client_ascii::render::map_tiles(s, &app.palette)),
                "palette":{"revision":app.palette.revision(),"stale":app.palette.stale()},
                "role":app.role,"travel":state.and_then(|s|s.travel()),"travel_cursor":app.travel_cursor,"door_direction":app.door_direction,
                "has_control":state.is_some_and(|s|s.has_control()),"can_admit_intention":state.is_some_and(|s|s.can_admit_intention()),"connected":app.connected,"busy":app.busy,"observation_current":state.map(|s|s.observation_current()),"character_ready":state.map(|s|s.character_ready()),
                "intentions":state.map(|s|s.intentions()),"action_hint":app.intention_hint(),
                "places_open":app.places_open,"place_selected":app.place_selected,"place_name":app.place_name,
                "narration":state.map(|s|s.narration())});
            let screen_report = serde_json::json!({
                "messages":{"shown":app.messages.shown(app.role!=tor_protocol::AccessRole::Spectator),"more":app.messages.more(),"turn":app.messages.text(),"log":app.messages.log().collect::<Vec<_>>()},
                "screen":{"pickup":!app.pickup.is_empty(),"inventory":app.inventory_open,"charts":app.charts_open,"help":app.help_open,"stats":app.stats_open,"stats_scroll":app.stats_scroll,"message_log":app.message_log,"look_cursor":app.look_cursor,"running":app.running},
                "inventory_rows":if app.inventory_open { state.map(|s|tor_client_ascii::inventory_entries(&app,&s.state().observation).iter().take(26).map(|entry|entry.long()).collect::<Vec<_>>()) } else { None },
                "chart_rows":if app.charts_open { Some(app.chart_lines()) } else { None },
                "stats_rows":if app.stats_open { Some(app.stats_rows()) } else { None },
                "advancement_open":app.advancement_open,"advancement_rows":app.advancement_rows(),"advancement_command":app.advancement_command,
                "ability_choices":app.ability_choices,"selected_ability":app.selected_ability,
                "item_rows":app.item_rows(),"zap_item":app.zap_item(),
                "identification":app.identification_choice(),"identification_rows":app.identification_rows(),
                "hold_choices":app.hold_choice_offers(),"hold_rows":app.hold_rows(),
                "supply_selection":app.supply_selection,"supply_rows":app.supply_rows(),
                "weapon_preview":app.weapon_confirmation,"weapon_preview_rows":app.weapon_preview_rows(),
                "projectile_selection":app.projectile_selection,"projectile_rows":app.projectile_rows(),
                "selected_weapon":app.selected_weapon,"weapon_selection":app.weapon_selection,"weapon_rows":app.weapon_rows(),
                "status_lines":state.map(|s|tor_client_ascii::status_lines(&app,s)),
                "rendered_status_lines":state.map(|s|tor_client_ascii::status_lines_at_width(&app,s,(tor_client_ascii::render::WIDTH-48)/16)),
                "status":app.status,"input_done":done,"native_input_sequence":native_input_sequence,
                "wizard_command":app.wizard_command,"command_line":app.command_line,
                "command_rows":app.command_rows(),"note":app.note.as_ref().map(|d|&d.text)});
            let serde_json::Value::Object(screen_fields) = screen_report else {
                unreachable!("the screen report is an object literal");
            };
            frame_report
                .as_object_mut()
                .expect("the frame report is an object literal")
                .extend(screen_fields);
            let encoded = frame_report.to_string();
            previous_report_encode_ms = encode_started.elapsed().as_secs_f64() * 1000.;
            let write_started = Instant::now();
            println!("{encoded}");
            io::stdout().flush()?;
            previous_report_write_ms = write_started.elapsed().as_secs_f64() * 1000.;
            previous_report_ms = report_started.elapsed().as_secs_f64() * 1000.;
        }
        if report {
            // Release is a native event-pump boundary, not a newly drawn frame.
            // Emit once per observed press and never repaint or report idle turns.
            for sequence in native_reports.after_pump(|key| window.is_key_down(key)) {
                println!(
                    "{}",
                    serde_json::json!({"type":"native_release","sequence":sequence})
                );
                io::stdout().flush()?;
            }
        }
        if quit {
            break;
        }
        if failed && report {
            return Err(app.status.into());
        }
    }
    if failed {
        Err(app.status.into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_editor_suppresses_typed_character_aliases_but_keeps_physical_navigation() {
        for native in [
            NativeKey::J,
            NativeKey::K,
            NativeKey::S,
            NativeKey::Z,
            NativeKey::Key3,
            NativeKey::Slash,
            NativeKey::Semicolon,
            NativeKey::Space,
            NativeKey::Period,
            NativeKey::Comma,
        ] {
            assert!(
                !command_editor_key(native),
                "typed {native:?} must be Text only"
            );
        }
        let character_events: Vec<_> = [NativeKey::K, NativeKey::J, NativeKey::Key3]
            .into_iter()
            .filter(|key| command_editor_key(*key))
            .filter_map(|key| native_key_with_modifiers(key, false, false))
            .collect();
        assert!(
            character_events.is_empty(),
            "typing #k must not select a menu row"
        );
        for native in [
            NativeKey::Up,
            NativeKey::Down,
            NativeKey::Enter,
            NativeKey::Escape,
            NativeKey::Backspace,
        ] {
            assert!(command_editor_key(native));
        }

        // minifb reports a printable key both as text and as a pressed key.
        // Reproduce that combined route: `#k<Enter>` is ambiguous, not #attack.
        let mut app = App::new();
        app.command_line = Some(String::new());
        assert_eq!(app.input(Input::Text { text: "k".into() }), Effect::None);
        for native in [NativeKey::K, NativeKey::Enter] {
            if app.command_line.is_none() || command_editor_key(native) {
                if let Some(key) = native_input_key(native, false, false, false, false) {
                    assert_eq!(app.input(Input::Key { key }), Effect::None);
                }
            }
        }
        assert_eq!(app.command_line.as_deref(), Some("k"));
        assert!(app.status.contains("ambiguous"));
    }

    #[test]
    fn native_navigation_and_modal_keys_map_to_tested_inputs() {
        for (native, expected) in [
            (NativeKey::A, Key::Apply),
            (NativeKey::B, Key::SouthWest),
            (NativeKey::C, Key::CloseDoor),
            (NativeKey::D, Key::Drop),
            (NativeKey::E, Key::Eat),
            (NativeKey::F, Key::Fire),
            (NativeKey::G, Key::RunPrefix),
            (NativeKey::H, Key::Left),
            (NativeKey::I, Key::Inventory),
            (NativeKey::J, Key::Down),
            (NativeKey::K, Key::Up),
            (NativeKey::L, Key::Right),
            (NativeKey::M, Key::MoveOnlyPrefix),
            (NativeKey::N, Key::SouthEast),
            (NativeKey::O, Key::OpenDoor),
            (NativeKey::Q, Key::Drink),
            (NativeKey::R, Key::Read),
            (NativeKey::S, Key::Search),
            (NativeKey::T, Key::Throw),
            (NativeKey::U, Key::NorthEast),
            (NativeKey::W, Key::Wield),
            (NativeKey::Y, Key::NorthWest),
            (NativeKey::Z, Key::Zap),
            (NativeKey::Up, Key::Up),
            (NativeKey::Down, Key::Down),
            (NativeKey::Left, Key::Left),
            (NativeKey::Right, Key::Right),
            (NativeKey::Period, Key::Wait),
            (NativeKey::Space, Key::Wait),
            (NativeKey::Comma, Key::Pickup),
            (NativeKey::Semicolon, Key::Look),
            (NativeKey::Slash, Key::Look),
        ] {
            assert_eq!(
                native_key_with_modifiers(native, false, false),
                Some(expected),
                "{native:?}"
            );
        }
        for (native, expected) in [
            (NativeKey::F2, Key::History),
            (NativeKey::F3, Key::Control),
            (NativeKey::F4, Key::Note),
            (NativeKey::F5, Key::Places),
            (NativeKey::F6, Key::Read),
            (NativeKey::F7, Key::Wizard),
            (NativeKey::F8, Key::ResumeIntention),
            (NativeKey::F9, Key::CancelIntention),
            (NativeKey::F10, Key::Advancement),
            (NativeKey::F11, Key::EscapeHold),
            (NativeKey::F12, Key::Zap),
            (NativeKey::Enter, Key::Enter),
            (NativeKey::Escape, Key::Escape),
            (NativeKey::Backspace, Key::Backspace),
            (NativeKey::Tab, Key::Tab),
            (NativeKey::PageUp, Key::OlderHistory),
            (NativeKey::PageDown, Key::RecentHistory),
        ] {
            for shift in [false, true] {
                assert_eq!(
                    native_key_with_modifiers(native, shift, false),
                    Some(expected),
                    "{native:?}, shift={shift}"
                );
            }
        }
        assert!(is_letter(NativeKey::J) && !is_letter(NativeKey::Enter));
    }

    #[test]
    fn native_shifted_commands_are_distinct_and_never_alias_lowercase_actions() {
        for (native, expected) in [
            (NativeKey::B, Key::RunSouthWest),
            (NativeKey::G, Key::RunPrefix),
            (NativeKey::H, Key::RunLeft),
            (NativeKey::J, Key::RunDown),
            (NativeKey::K, Key::RunUp),
            (NativeKey::L, Key::RunRight),
            (NativeKey::N, Key::RunSouthEast),
            (NativeKey::P, Key::PutOn),
            (NativeKey::R, Key::RemoveAccessory),
            (NativeKey::S, Key::SaveQuit),
            (NativeKey::T, Key::TakeOff),
            (NativeKey::U, Key::RunNorthEast),
            (NativeKey::W, Key::Wear),
            (NativeKey::Y, Key::RunNorthWest),
            (NativeKey::Z, Key::Abilities),
            (NativeKey::Up, Key::RunUp),
            (NativeKey::Down, Key::RunDown),
            (NativeKey::Left, Key::RunLeft),
            (NativeKey::Right, Key::RunRight),
            (NativeKey::Comma, Key::Ascend),
            (NativeKey::Period, Key::Descend),
            (NativeKey::Slash, Key::Help),
            (NativeKey::Key3, Key::Commands),
        ] {
            assert_eq!(
                native_key_with_modifiers(native, true, false),
                Some(expected),
                "{native:?}"
            );
        }
        // Unsupported NetHack commands must not perform their former TOR action.
        for native in [
            NativeKey::A,
            NativeKey::C,
            NativeKey::D,
            NativeKey::E,
            NativeKey::F,
            NativeKey::I,
            NativeKey::M,
            NativeKey::O,
            NativeKey::Q,
            NativeKey::V,
            NativeKey::X,
            NativeKey::Key2,
            NativeKey::Semicolon,
            NativeKey::LeftBracket,
            NativeKey::RightBracket,
        ] {
            assert_eq!(
                native_key_with_modifiers(native, true, false),
                None,
                "{native:?}"
            );
        }
        for native in [
            NativeKey::P,
            NativeKey::V,
            NativeKey::X,
            NativeKey::Key2,
            NativeKey::Key3,
            NativeKey::LeftBracket,
            NativeKey::RightBracket,
            NativeKey::LeftShift,
            NativeKey::RightShift,
        ] {
            assert_eq!(
                native_key_with_modifiers(native, false, false),
                None,
                "{native:?}"
            );
        }
    }

    #[test]
    fn native_control_commands_and_unsupported_alt_never_trigger_plain_actions() {
        for (native, expected) in [
            (NativeKey::P, Key::MessageLog),
            (NativeKey::X, Key::Stats),
            (NativeKey::O, Key::Places),
        ] {
            for shift in [false, true] {
                assert_eq!(
                    native_key_with_modifiers(native, shift, true),
                    Some(expected)
                );
            }
        }
        for native in [
            NativeKey::A,
            NativeKey::K,
            NativeKey::Q,
            NativeKey::S,
            NativeKey::T,
            NativeKey::Z,
            NativeKey::Comma,
            NativeKey::F6,
        ] {
            assert_eq!(
                native_key_with_modifiers(native, false, true),
                None,
                "{native:?}"
            );
        }
        for native in [
            NativeKey::Q,
            NativeKey::A,
            NativeKey::S,
            NativeKey::X,
            NativeKey::F6,
            NativeKey::LeftBracket,
        ] {
            for shift in [false, true] {
                for ctrl in [false, true] {
                    assert_eq!(
                        native_input_key(native, shift, ctrl, true, true),
                        None,
                        "{native:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn native_brackets_page_only_an_open_strike_detail() {
        for (native, expected) in [
            (NativeKey::LeftBracket, Key::Slower),
            (NativeKey::RightBracket, Key::Faster),
        ] {
            assert_eq!(
                native_input_key(native, false, false, false, true),
                Some(expected)
            );
            assert_eq!(native_input_key(native, false, false, false, false), None);
            assert_eq!(native_input_key(native, true, false, false, true), None);
            assert_eq!(native_input_key(native, false, true, false, true), None);
        }
    }
}
