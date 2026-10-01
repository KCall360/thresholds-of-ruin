mod network;

use minifb::{InputCallback, Key as NativeKey, KeyRepeat, ScaleMode, Window, WindowOptions};
use network::{Event, Network};
use std::{
    cell::RefCell,
    io::{self, BufRead, Write},
    net::SocketAddr,
    path::PathBuf,
    rc::Rc,
    sync::mpsc,
    time::Instant,
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

fn native_key_with_shift(key: NativeKey, shift: bool, ctrl: bool) -> Option<Key> {
    if ctrl && key == NativeKey::P {
        return Some(Key::Scrollback);
    }
    if key == NativeKey::F9 {
        return Some(Key::Quit);
    }
    if let Some(mapped) = numpad_key(key) {
        return Some(mapped);
    }
    if shift {
        if let Some(mapped) = shifted_key(key) {
            return Some(mapped);
        }
    }
    native_key(key)
}

fn numpad_key(key: NativeKey) -> Option<Key> {
    Some(match key {
        NativeKey::NumPad0 => Key::Numpad0,
        NativeKey::NumPad1 => Key::Numpad1,
        NativeKey::NumPad2 => Key::Numpad2,
        NativeKey::NumPad3 => Key::Numpad3,
        NativeKey::NumPad4 => Key::Numpad4,
        NativeKey::NumPad5 => Key::Numpad5,
        NativeKey::NumPad6 => Key::Numpad6,
        NativeKey::NumPad7 => Key::Numpad7,
        NativeKey::NumPad8 => Key::Numpad8,
        NativeKey::NumPad9 => Key::Numpad9,
        _ => return None,
    })
}

fn shifted_key(key: NativeKey) -> Option<Key> {
    Some(match key {
        NativeKey::Comma => Key::Ascend,
        NativeKey::Period => Key::Descend,
        NativeKey::Key2 => Key::Autopickup,
        NativeKey::Key3 => Key::Extended,
        NativeKey::Key4 => Key::Letter('$'),
        NativeKey::Key7 => Key::KeyHelp,
        NativeKey::Slash => Key::Help,
        NativeKey::Semicolon => Key::Describe,
        NativeKey::F => Key::Fight,
        NativeKey::M => Key::SuppressRun,
        NativeKey::D => Key::DropMany,
        NativeKey::Y => Key::RunNorthWest,
        NativeKey::U => Key::RunNorthEast,
        NativeKey::H => Key::RunWest,
        NativeKey::J => Key::RunSouth,
        NativeKey::K => Key::RunNorth,
        NativeKey::L => Key::RunEast,
        NativeKey::B => Key::RunSouthWest,
        NativeKey::N => Key::RunSouthEast,
        other => return alphabetic(other, true),
    })
}

fn native_key(key: NativeKey) -> Option<Key> {
    if let Some(mapped) = numpad_key(key) {
        return Some(mapped);
    }
    Some(match key {
        NativeKey::Up | NativeKey::K => Key::Up,
        NativeKey::Down | NativeKey::J => Key::Down,
        NativeKey::Left | NativeKey::H => Key::Left,
        NativeKey::Right | NativeKey::L => Key::Right,
        NativeKey::Y => Key::NorthWest,
        NativeKey::U => Key::NorthEast,
        NativeKey::B => Key::SouthWest,
        NativeKey::N => Key::SouthEast,
        NativeKey::Space => Key::Space,
        NativeKey::Period => Key::Wait,
        NativeKey::Comma => Key::Pickup,
        NativeKey::G => Key::Go,
        NativeKey::D => Key::Drop,
        NativeKey::O => Key::OpenDoor,
        NativeKey::C => Key::CloseDoor,
        NativeKey::I => Key::Inventory,
        NativeKey::M => Key::Suppress,
        NativeKey::F3 => Key::Control,
        NativeKey::R => Key::Release,
        NativeKey::F4 => Key::Note,
        NativeKey::F5 => Key::Places,
        NativeKey::Enter => Key::Enter,
        NativeKey::Escape => Key::Escape,
        NativeKey::Backspace => Key::Backspace,
        NativeKey::Tab => Key::Tab,
        NativeKey::F2 => Key::History,
        NativeKey::F9 => Key::Quit,
        NativeKey::PageUp => Key::OlderHistory,
        NativeKey::PageDown => Key::RecentHistory,
        NativeKey::Key0 => Key::Key0,
        NativeKey::Key1 => Key::Key1,
        NativeKey::Key2 => Key::Key2,
        NativeKey::Key3 => Key::Key3,
        NativeKey::Key4 => Key::Key4,
        NativeKey::Key5 => Key::Key5,
        NativeKey::Key6 => Key::Key6,
        NativeKey::Key7 => Key::Key7,
        NativeKey::Key8 => Key::Key8,
        NativeKey::Key9 => Key::Key9,
        other => return alphabetic(other, false),
    })
}

fn alphabetic(key: NativeKey, shift: bool) -> Option<Key> {
    let lower = match key {
        NativeKey::A => 'a',
        NativeKey::B => 'b',
        NativeKey::C => 'c',
        NativeKey::D => 'd',
        NativeKey::E => 'e',
        NativeKey::F => 'f',
        NativeKey::G => 'g',
        NativeKey::H => 'h',
        NativeKey::I => 'i',
        NativeKey::J => 'j',
        NativeKey::K => 'k',
        NativeKey::L => 'l',
        NativeKey::M => 'm',
        NativeKey::N => 'n',
        NativeKey::O => 'o',
        NativeKey::P => 'p',
        NativeKey::Q => 'q',
        NativeKey::R => 'r',
        NativeKey::S => 's',
        NativeKey::T => 't',
        NativeKey::U => 'u',
        NativeKey::V => 'v',
        NativeKey::W => 'w',
        NativeKey::X => 'x',
        NativeKey::Y => 'y',
        NativeKey::Z => 'z',
        _ => return None,
    };
    let ch = if shift {
        lower.to_ascii_uppercase()
    } else {
        lower
    };
    Some(Key::Letter(ch))
}

fn text_key(key: Key) -> bool {
    matches!(
        key,
        Key::Letter(_)
            | Key::Backspace
            | Key::Key0
            | Key::Key1
            | Key::Key2
            | Key::Key3
            | Key::Key4
            | Key::Key5
            | Key::Key6
            | Key::Key7
            | Key::Key8
            | Key::Key9
            | Key::Numpad0
            | Key::Numpad1
            | Key::Numpad2
            | Key::Numpad3
            | Key::Numpad4
            | Key::Numpad5
            | Key::Numpad6
            | Key::Numpad7
            | Key::Numpad8
            | Key::Numpad9
    )
}

fn submit(app: &mut App, network: &Network, effect: Effect) -> bool {
    let Effect::Request(request) = effect else {
        return false;
    };
    match network.commands.try_send(request) {
        Ok(()) => false,
        Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
            app.unsend();
            false
        }
        Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
            app.disconnect("Network command queue unavailable.".into());
            true
        }
    }
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
    let mut config = tor_client_ascii::SessionConfig::default();
    let mut automation = false;
    let mut report = false;
    let mut capture = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("tor-client-ascii [--connect 127.0.0.1:4000] [--actor 1] [--observe] [--config file.toml]\nSet TOR_SERVER_TOKEN to the server token. A native graphical display is required.\n--config reads autopickup, click (travel|look), and bump_attacks (hostile|any|off). Omitted, those default to on, travel, and hostile.\nArrows/hjkl/yubn and the numpad: move. Shifted YUHJKLBN: run. .: wait. F: fight. o/c then a direction: door. ,: pickup. d: drop. i: inventory. g: run. m: one step without a bump attack or autopickup. _: travel. @: autopickup. Left click: travel or look from the config file. F3/R: acquire/release control.\nF5: remembered places (Up/Down select, Enter rename); F4: note (Tab audience, Enter save, Esc cancel); F2: history (Up/Down scroll, PgUp older, PgDn live).\nEsc cancels the current mode or travel and does not quit. F9 quits. Space confirms a menu or --More-- and does not wait. S does not save.\nProcess tests only: --automation reads JSON input events on stdin and reports presented frames.\n--report-frames reports frames while retaining native keyboard input.\n--capture <file.ppm> with either diagnostic option saves the last presented framebuffer.");
                return Ok(());
            }
            "--connect" => address = args.next().ok_or("Missing --connect address")?.parse()?,
            "--actor" => actor = ActorId(args.next().ok_or("Missing --actor ID")?.parse()?),
            "--observe" => observe = true,
            "--config" => {
                let path = args.next().ok_or("Missing --config path")?;
                config = tor_client_hack::load_config(std::path::Path::new(&path))?;
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
    let network = Network::start(address, token, actor, observe);
    let result = window_loop(&mut window, &network, &text, input, report, capture, config);
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
    config: tor_client_ascii::SessionConfig,
) -> Result<(), Error> {
    let mut app = App::new();
    app.config = config;
    let mut canvas = Canvas::default();
    let mut frame = 0u64;
    let mut pending_input = None;
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
                        Event::Snapshot(snapshot) => app
                            .replace_snapshot(*snapshot)
                            .map_err(|e| format!("Invalid presentation snapshot: {e:?}"))?,
                        Event::Update(update) => app
                            .update(*update)
                            .map_err(|e| format!("Invalid presentation update: {e:?}"))?,
                        Event::Status(status) => app.accept_status(status),
                        Event::Ready => app.ready(),
                        Event::History(page) => {
                            app.history_scroll = 0;
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
        let pumped = app.pump();
        if submit(&mut app, network, pumped) {
            failed = true;
        }
        let mut inputs = Vec::new();
        let typed = std::mem::take(&mut *text.borrow_mut());
        let text_for_editor = !typed.is_empty() && app.accepts_text();
        if input.is_none() {
            if text_for_editor {
                inputs.push(Input::Text {
                    text: typed.clone(),
                });
            }
            if app.accepts_travel_chord() && typed.contains('_') {
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
            inputs.extend(
                window
                    .get_keys_pressed(KeyRepeat::No)
                    .into_iter()
                    .filter_map(|key| {
                        native_key_with_shift(
                            key,
                            window.is_key_down(NativeKey::LeftShift)
                                || window.is_key_down(NativeKey::RightShift),
                            window.is_key_down(NativeKey::LeftCtrl)
                                || window.is_key_down(NativeKey::RightCtrl),
                        )
                    })
                    .filter(|key| !text_for_editor || !text_key(*key))
                    .map(|key| Input::Key { key }),
            );
        } else if let Some(input) = input.as_ref() {
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
        for event in inputs {
            dirty = true;
            match app.input(event) {
                Effect::Quit => {
                    quit = true;
                    break;
                }
                other => {
                    if submit(&mut app, network, other) {
                        failed = true;
                    }
                }
            }
        }
        let pumped = app.pump();
        if submit(&mut app, network, pumped) {
            failed = true;
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
            // A key is done when its command, and any pickup it caused, has left
            // the queue. A repeat stays queued until its remaining steps finish.
            let done = if !app.busy && app.queued() == 0 {
                pending_input.take()
            } else {
                None
            };
            let state = app.state.as_ref();
            let capture_started = Instant::now();
            if let Some(path) = &capture {
                save_frame(path, &canvas)?;
            }
            let capture_ms = capture_started.elapsed().as_secs_f64() * 1000.;
            let encode_started = Instant::now();
            let encoded = serde_json::json!({"type":"frame","frame":frame,
                "presented_unix_ns":presented_unix_ns,
                "profile":{"version":1,"network_events":network_events,"apply_ms":apply_ms,"draw_ms":draw_ms,
                "previous_report_encode_ms":previous_report_encode_ms,"previous_report_write_ms":previous_report_write_ms,
                "native_ms":native_ms,"capture_ms":capture_ms,"previous_report_ms":previous_report_ms,"turn_interval_ms":turn_interval_ms},"window_open":window.is_open(),
                "state":state.map(|s|s.state()),"branch":state.map(|s|s.branch()),"history":state.map(|s|s.history()),
                "map_tiles":tor_client_ascii::render::map_tiles(&app),
                "status_lines":tor_client_ascii::render::status_lines(&app),
                "role":app.role,"travel":state.and_then(|s|s.travel()),"travel_cursor":app.travel_cursor,"door_direction":app.door_direction,
                "has_control":state.is_some_and(|s|s.has_control()),"connected":app.connected,"busy":app.busy,
                "places_open":app.places_open,"place_selected":app.place_selected,"place_name":app.place_name,
                "narration":state.map(|s|s.narration()),
                "messages":app.message_lines(),"more":app.more(),
                "inventory_letters":app.inventory_letters(),"queued":app.queued(),"look_cursor":app.look_cursor(),
                "status":app.status,"input_done":done,"note":app.note.as_ref().map(|d|&d.text)}).to_string();
            previous_report_encode_ms = encode_started.elapsed().as_secs_f64() * 1000.;
            let write_started = Instant::now();
            println!("{encoded}");
            io::stdout().flush()?;
            previous_report_write_ms = write_started.elapsed().as_secs_f64() * 1000.;
            previous_report_ms = report_started.elapsed().as_secs_f64() * 1000.;
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

fn save_frame(path: &std::path::Path, canvas: &Canvas) -> io::Result<()> {
    let mut output = io::BufWriter::new(std::fs::File::create(path)?);
    write!(output, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    for pixel in &canvas.pixels {
        output.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8])?;
    }
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_navigation_and_modal_keys_map_to_tested_inputs() {
        assert_eq!(native_key(NativeKey::Right), Some(Key::Right));
        assert_eq!(native_key(NativeKey::L), Some(Key::Right));
        assert_eq!(native_key(NativeKey::O), Some(Key::OpenDoor));
        assert_eq!(native_key(NativeKey::C), Some(Key::CloseDoor));
        assert_eq!(native_key(NativeKey::F3), Some(Key::Control));
        assert_eq!(native_key(NativeKey::P), Some(Key::Letter('p')));
        assert_eq!(native_key(NativeKey::Space), Some(Key::Space));
        assert_eq!(
            native_key_with_shift(NativeKey::P, false, true),
            Some(Key::Scrollback)
        );
        assert_eq!(
            native_key_with_shift(NativeKey::P, false, false),
            Some(Key::Letter('p'))
        );
        assert_eq!(native_key(NativeKey::G), Some(Key::Go));
        assert_eq!(native_key(NativeKey::Comma), Some(Key::Pickup));
        assert_eq!(native_key(NativeKey::A), Some(Key::Letter('a')));
        assert_eq!(native_key(NativeKey::F9), Some(Key::Quit));
        assert_eq!(
            native_key_with_shift(NativeKey::F, true, false),
            Some(Key::Fight)
        );
        assert_eq!(native_key(NativeKey::F), Some(Key::Letter('f')));
        assert_eq!(native_key(NativeKey::NumPad6), Some(Key::Numpad6));
        assert_eq!(native_key(NativeKey::NumPad5), Some(Key::Numpad5));
        assert_eq!(native_key(NativeKey::NumPad0), Some(Key::Numpad0));
        assert_eq!(native_key(NativeKey::Key3), Some(Key::Key3));
        assert_eq!(
            native_key_with_shift(NativeKey::L, true, false),
            Some(Key::RunEast)
        );
        assert_eq!(native_key(NativeKey::N), Some(Key::SouthEast));
        assert_eq!(native_key(NativeKey::F4), Some(Key::Note));
        assert_eq!(native_key(NativeKey::F5), Some(Key::Places));
        assert_eq!(native_key(NativeKey::Escape), Some(Key::Escape));
        assert_eq!(native_key(NativeKey::F2), Some(Key::History));
        assert_eq!(native_key(NativeKey::LeftShift), None);
        assert_eq!(native_key(NativeKey::Y), Some(Key::NorthWest));
        assert_eq!(native_key(NativeKey::U), Some(Key::NorthEast));
        assert_eq!(native_key(NativeKey::B), Some(Key::SouthWest));
        assert_eq!(
            native_key_with_shift(NativeKey::Comma, true, false),
            Some(Key::Ascend)
        );
        assert_eq!(
            native_key_with_shift(NativeKey::Period, true, false),
            Some(Key::Descend)
        );
        assert_eq!(
            native_key_with_shift(NativeKey::Period, false, false),
            Some(Key::Wait)
        );
    }
}
