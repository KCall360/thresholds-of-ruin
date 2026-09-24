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

fn native_key_with_shift(key: NativeKey, shift: bool) -> Option<Key> {
    match (key, shift) {
        (NativeKey::Comma, true) => Some(Key::Ascend),
        (NativeKey::Period, true) => Some(Key::Descend),
        _ => native_key(key),
    }
}

fn native_key(key: NativeKey) -> Option<Key> {
    Some(match key {
        NativeKey::Up | NativeKey::K => Key::Up,
        NativeKey::Down | NativeKey::J => Key::Down,
        NativeKey::Left | NativeKey::H => Key::Left,
        NativeKey::Right | NativeKey::L => Key::Right,
        NativeKey::Y => Key::NorthWest,
        NativeKey::U => Key::NorthEast,
        NativeKey::B => Key::SouthWest,
        NativeKey::N => Key::SouthEast,
        NativeKey::D => Key::Descend,
        NativeKey::Space | NativeKey::Period => Key::Wait,
        NativeKey::G => Key::Pickup,
        NativeKey::O => Key::OpenDoor,
        NativeKey::C => Key::CloseDoor,
        NativeKey::F3 => Key::Control,
        NativeKey::R => Key::Release,
        NativeKey::F4 => Key::Note,
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
    let mut automation = false;
    let mut report = false;
    let mut capture = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("tor-client-ascii [--connect 127.0.0.1:4000] [--actor 1] [--observe]\nSet TOR_SERVER_TOKEN to the server token. A native graphical display is required.\nArrows/HJKL/YUBN: move; </>: up/down; Space: wait; G: pickup; O/C then direction: open/close adjacent door; _: select travel destination; left click: travel; F3/R: acquire/release control.\nF4: note (Tab audience, Enter save, Esc cancel); F2: history (Up/Down scroll, PgUp older, PgDn live).\nEsc: cancel selection/travel, close modal, or quit. Relaunch to reconnect after a disconnect.\nProcess tests only: --automation reads JSON input events on stdin and reports presented frames.\n--report-frames reports frames while retaining native keyboard input.\n--capture <file.ppm> with either diagnostic option saves the last presented framebuffer.");
                return Ok(());
            }
            "--connect" => address = args.next().ok_or("Missing --connect address")?.parse()?,
            "--actor" => actor = ActorId(args.next().ok_or("Missing --actor ID")?.parse()?),
            "--observe" => observe = true,
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
    window.set_target_fps(30);
    let text = Rc::new(RefCell::new(String::new()));
    window.set_input_callback(Box::new(TextInput(text.clone())));
    let input = automation.then(automation_input);
    let network = Network::start(address, token, actor, observe);
    let result = window_loop(&mut window, &network, &text, input, report, capture);
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
) -> Result<(), Error> {
    let mut app = App::new();
    let mut canvas = Canvas::default();
    let mut frame = 0u64;
    let mut pending_input = None;
    let mut failed = false;
    let mut mouse_down = false;
    while window.is_open() {
        let mut dirty = frame == 0;
        loop {
            match network.events.try_recv() {
                Ok(event) => {
                    dirty = true;
                    match event {
                        Event::Role(role) => app.role = role,
                        Event::State(state) => app.set_state(*state),
                        Event::Status(status) => app.status = status,
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
        let mut inputs = Vec::new();
        let typed = std::mem::take(&mut *text.borrow_mut());
        if input.is_none() {
            // Send typed text only to an already-open note editor.
            if app.note.is_some() && !typed.is_empty() {
                inputs.push(Input::Text {
                    text: typed.clone(),
                });
            }
            if app.note.is_none() && typed.contains('_') {
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
                        )
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
        for event in inputs {
            dirty = true;
            match app.input(event) {
                Effect::None => {}
                Effect::Quit => {
                    quit = true;
                    break;
                }
                Effect::Request(request) => {
                    if network.commands.try_send(request).is_err() {
                        app.disconnect("Network command queue unavailable.".into());
                        failed = true;
                    }
                }
            }
        }
        if dirty {
            canvas.draw(&app);
            window.update_with_buffer(&canvas.pixels, WIDTH, HEIGHT)?;
            frame += 1;
        } else {
            // Pump native events without repainting an unchanged 960,000-pixel
            // framebuffer. State/input changes set `dirty` above.
            window.update();
        }
        // This is intentionally after real native presentation. There is no
        // headless fallback; CI must supply a functioning display environment.
        if report && dirty {
            let done = if !app.busy {
                pending_input.take()
            } else {
                None
            };
            let state = app.state.as_ref();
            if let Some(path) = &capture {
                save_frame(path, &canvas)?;
            }
            println!(
                "{}",
                serde_json::json!({"type":"frame","frame":frame,"window_open":window.is_open(),
                "state":state.map(|s|s.state()),"branch":state.map(|s|s.branch()),"history":state.map(|s|s.history()),
                "map_tiles":state.map(tor_client_ascii::render::map_tiles),
                "role":app.role,"travel":state.and_then(|s|s.travel()),"travel_cursor":app.travel_cursor,"door_direction":app.door_direction,
                "has_control":state.is_some_and(|s|s.has_control()),"connected":app.connected,"busy":app.busy,
                "status":app.status,"input_done":done,"note":app.note.as_ref().map(|d|&d.text)})
            );
            io::stdout().flush()?;
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
        assert_eq!(native_key(NativeKey::P), None);
        assert_eq!(native_key(NativeKey::G), Some(Key::Pickup));
        assert_eq!(native_key(NativeKey::N), Some(Key::SouthEast));
        assert_eq!(native_key(NativeKey::F4), Some(Key::Note));
        assert_eq!(native_key(NativeKey::Escape), Some(Key::Escape));
        assert_eq!(native_key(NativeKey::F2), Some(Key::History));
        assert_eq!(native_key(NativeKey::LeftShift), None);
        assert_eq!(native_key(NativeKey::Y), Some(Key::NorthWest));
        assert_eq!(native_key(NativeKey::U), Some(Key::NorthEast));
        assert_eq!(native_key(NativeKey::B), Some(Key::SouthWest));
        assert_eq!(
            native_key_with_shift(NativeKey::Comma, true),
            Some(Key::Ascend)
        );
        assert_eq!(
            native_key_with_shift(NativeKey::Period, true),
            Some(Key::Descend)
        );
        assert_eq!(
            native_key_with_shift(NativeKey::Period, false),
            Some(Key::Wait)
        );
    }
}
