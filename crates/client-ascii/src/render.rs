//! Fixed logical canvas, scaled by the native window without changing game state.
//!
//! Layout, as in NetHack: message and prompt lines across the top, the map
//! across the full width, two status lines at the bottom. Everything else
//! (inventory, creature details, help, logs) opens on demand over the map.
use crate::map::{self, Kind};
use crate::{history_lines, App};
use font8x8::{UnicodeFonts, BASIC_FONTS};
use tor_protocol::{AccessRole, Observation};

pub use crate::map::{MapTile, MEMORY_COLOR};

pub const WIDTH: usize = 1200;
pub const HEIGHT: usize = 800;
const BG: u32 = 0x0c1118;
const PANEL: u32 = 0x131d27;
const BORDER: u32 = 0x263747;
const TEXT: u32 = 0xd9e3e9;
const MUTED: u32 = 0x869ba9;
const ACCENT: u32 = 0x67d8bd;
const GOLD: u32 = 0xedc579;
const DANGER: u32 = 0xef958c;

/// The key hint under the status lines.
const HINT: &str = "? help  hjklyubn move (shift: run)  < > stairs  _ travel  ; look  i inventory  g pick up  a attack  ^P messages";

/// Current cells plus remembered ones that fall inside the drawn map.
fn display_observation(state: &tor_client_common::ClientState) -> Observation {
    let mut view = state.state().observation.clone();
    let visible: std::collections::BTreeSet<_> = view
        .visible_cells
        .iter()
        .map(|c| (c.position.x, c.position.y, c.position.z))
        .collect();
    for cell in state.map_memory() {
        let p = cell.position;
        if !map::in_view(&view, p) || visible.contains(&(p.x, p.y, p.z)) {
            continue;
        }
        view.visible_cells.push(cell.cell_view());
        view.ground_items.extend(cell.ground_items.iter().cloned());
    }
    view
}

/// The map as drawn: one tile per column, current and remembered.
pub fn map_tiles(state: &tor_client_common::ClientState) -> Vec<MapTile> {
    let current: std::collections::BTreeSet<_> = state
        .state()
        .observation
        .visible_cells
        .iter()
        .map(|c| (c.position.x, c.position.y, c.position.z))
        .collect();
    map::tiles(&display_observation(state), &|cell| {
        current.contains(&(cell.position.x, cell.position.y, cell.position.z))
    })
}

/// The known cell under a pixel, current or remembered.
pub fn known_cell_at(
    state: &tor_client_common::ClientState,
    x: usize,
    y: usize,
) -> Option<tor_protocol::Position> {
    let position = map::cell_at(&display_observation(state), x, y)?;
    state.map_cell(position).map(|c| c.position)
}

/// The known cell a column selects, current or remembered.
pub fn known_column(
    state: &tor_client_common::ClientState,
    x: i32,
    y: i32,
) -> Option<tor_protocol::Position> {
    let position = map::target(&display_observation(state), x, y)?;
    state.map_cell(position).map(|c| c.position)
}

/// Hit testing shares the exact layout used to draw cells.
pub fn cell_at(o: &Observation, x: usize, y: usize) -> Option<tor_protocol::Position> {
    map::cell_at(o, x, y)
}

pub fn cell_center(o: &Observation, position: tor_protocol::Position) -> Option<(usize, usize)> {
    map::cell_center(o, position)
}

pub struct Canvas {
    pub pixels: Vec<u32>,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            pixels: vec![BG; WIDTH * HEIGHT],
        }
    }
}

impl Canvas {
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        for py in y..y.saturating_add(h).min(HEIGHT) {
            for px in x..x.saturating_add(w).min(WIDTH) {
                self.pixels[py * WIDTH + px] = color;
            }
        }
    }
    fn outline(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        self.rect(x, y, w, 2, color);
        self.rect(x, y + h - 2, w, 2, color);
        self.rect(x, y, 2, h, color);
        self.rect(x + w - 2, y, 2, h, color);
    }
    fn text(&mut self, x: usize, y: usize, text: &str, color: u32, scale: usize, limit: usize) {
        let clipped = text.chars().count() > limit;
        for (i, ch) in text.chars().take(limit).enumerate() {
            let ch = if clipped && i + 1 == limit { '~' } else { ch };
            let ch = if matches!(ch, '—' | '–') { '-' } else { ch };
            let glyph = BASIC_FONTS
                .get(ch)
                .or_else(|| BASIC_FONTS.get('?'))
                .unwrap_or([0; 8]);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        self.rect(
                            x + i * 8 * scale + col * scale,
                            y + row * scale,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
        }
    }
    fn panel(&mut self, x: usize, y: usize, w: usize, h: usize) {
        self.rect(x, y, w, h, BORDER);
        self.rect(x + 1, y + 1, w - 2, h - 2, PANEL);
    }
    /// A titled screen over the main view.
    fn screen(&mut self, title: &str, keys: &str) {
        self.panel(48, 76, 1104, 644);
        self.text(72, 96, title, ACCENT, 2, 65);
        self.text(72, 124, keys, MUTED, 1, 128);
    }

    pub fn draw(&mut self, app: &App) {
        self.pixels.fill(BG);
        self.draw_messages(app);
        if let Some(state) = &app.state {
            self.draw_map(app, state);
        } else {
            self.text(40, 100, "Connecting...", MUTED, 2, 40);
        }
        self.draw_status(app);
        self.draw_screens(app);
    }

    /// Messages, then any prompt or refusal on the next row, as NetHack
    /// shows prompts on its message line.
    fn draw_messages(&mut self, app: &App) {
        let paging = app.role != AccessRole::Spectator;
        let rows = app.messages.shown(paging);
        for (i, row) in rows.iter().enumerate() {
            self.text(24, 6 + i * 20, row, TEXT, 2, crate::messages::WIDTH);
        }
        // A bare acknowledgement says nothing the player needs.
        if !app.busy && !app.status.is_empty() && app.status != "Done." && !app.messages.more() {
            self.text(
                24,
                6 + rows.len().min(crate::messages::ROWS) * 20,
                &app.status,
                GOLD,
                2,
                crate::messages::WIDTH,
            );
        }
    }

    fn draw_map(&mut self, app: &App, state: &tor_client_common::ClientState) {
        let o = &state.state().observation;
        let cursor = app.travel_cursor.or(app.look_cursor);
        let target = app
            .attack_targets
            .get(app.selected)
            .map(|actor| (actor.position.x, actor.position.y));
        for tile in map_tiles(state) {
            let column = (tile.position.x, tile.position.y);
            let selected = cursor.is_some_and(|c| (c.x, c.y) == column);
            let x = tile.center.0 - tile.step / 2;
            let y = tile.center.1 - tile.step / 2;
            self.rect(
                x,
                y,
                tile.step - 1,
                tile.step - 1,
                if selected {
                    GOLD
                } else if tile.remembered {
                    0x151a20
                } else if tile.kind == Kind::Player {
                    0x203f41
                } else {
                    0x182430
                },
            );
            self.text(
                x + 1,
                y + 1,
                &tile.glyph.to_string(),
                if selected { BG } else { tile.color },
                2,
                1,
            );
            if target == Some(column) && tile.kind == Kind::Creature {
                self.outline(x, y, tile.step, tile.step, DANGER);
            }
        }
        // A cursor over an unknown column is still shown.
        if let Some(cursor) = cursor {
            let dx = cursor.x - o.position.x + map::COLS / 2;
            let dy = cursor.y - o.position.y + map::ROWS / 2;
            if (0..map::COLS).contains(&dx) && (0..map::ROWS).contains(&dy) {
                self.outline(
                    map::LEFT + dx as usize * map::STEP,
                    map::TOP + dy as usize * map::STEP,
                    map::STEP,
                    map::STEP,
                    GOLD,
                );
            }
        }
    }

    /// Two status lines, as in NetHack, and a one-line key hint.
    fn draw_status(&mut self, app: &App) {
        if let Some(state) = &app.state {
            let [first, second] = crate::status_lines(app, state);
            self.text(24, 700, &first, TEXT, 2, 72);
            self.text(24, 724, &second, TEXT, 2, 72);
        }
        let hint = if app.role == AccessRole::Spectator {
            "READ-ONLY   ? help   ^P messages   F2 history   F5 places   Esc quit".into()
        } else {
            app.intention_hint().unwrap_or_else(|| HINT.into())
        };
        self.text(24, 760, &hint, MUTED, 1, 146);
        self.text(
            24,
            778,
            &crate::control_label(app),
            if app.busy { GOLD } else { ACCENT },
            1,
            60,
        );
    }

    fn draw_screens(&mut self, app: &App) {
        if let Some(state) = &app.state {
            let combat = state.state().observation.combat.as_ref();
            if let Some(combat) = combat.filter(|c| c.terminal && !app.end_dismissed) {
                self.panel(220, 250, 760, 200);
                self.text(
                    252,
                    276,
                    if combat.dead { "YOU DIED" } else { "VICTORY" },
                    if combat.dead { DANGER } else { GOLD },
                    3,
                    20,
                );
                for (i, line) in crate::end_summary(state).iter().enumerate() {
                    self.text(252, 320 + i * 22, line, TEXT, 2, 44);
                }
                self.text(
                    252,
                    424,
                    "Enter closes this   ^P messages   F2 history   Esc quits",
                    MUTED,
                    1,
                    80,
                );
            }
        }
        if let Some(draft) = &app.note {
            self.panel(60, 180, 1080, 424);
            self.text(
                84,
                206,
                &format!("NEW NOTE / {:?}", draft.audience),
                ACCENT,
                2,
                60,
            );
            self.text(
                84,
                240,
                "TAB audience   ENTER save   ESC cancel",
                MUTED,
                1,
                80,
            );
            let lines = crate::wrap(&draft.text, 64);
            let start = lines.len().saturating_sub(11);
            for (i, line) in lines.iter().skip(start).enumerate() {
                self.text(84, 274 + i * 26, line, TEXT, 2, 64);
            }
        }
        if let Some(open) = app.door_direction {
            self.panel(160, 176, 880, 160);
            self.text(
                188,
                204,
                if open {
                    "OPEN IN WHICH DIRECTION?"
                } else {
                    "CLOSE IN WHICH DIRECTION?"
                },
                ACCENT,
                2,
                50,
            );
            self.text(188, 250, "hjklyubn or arrow keys", TEXT, 2, 50);
            self.text(188, 292, "Esc cancels without taking a turn", MUTED, 1, 80);
        }
        if !app.pickup.is_empty() {
            self.panel(160, 150, 880, 480);
            let title = match app.item_operation {
                Some(operation) => format!("WHAT DO YOU WANT TO {}?", operation.verb()),
                None if app.dropping => "WHAT DO YOU WANT TO DROP?".into(),
                None => "WHAT DO YOU WANT TO PICK UP?".into(),
            };
            self.text(188, 176, &title.to_uppercase(), ACCENT, 2, 50);
            self.text(
                188,
                208,
                &if app.item_operation.is_some() {
                    "letter or UP/DOWN + ENTER chooses   ESC cancels".to_owned()
                } else {
                    format!(
                        "letter or UP/DOWN + ENTER chooses   digits set a count (now: {})   ESC cancels",
                        if app.quantity.is_empty() {
                            "all"
                        } else {
                            &app.quantity
                        }
                    )
                },
                MUTED,
                1,
                100,
            );
            let start = app.selected.saturating_sub(10);
            for (i, item) in app.pickup.iter().enumerate().skip(start).take(11) {
                self.text(
                    188,
                    240 + (i - start) * 32,
                    &format!(
                        "{} {} - {}",
                        if i == app.selected { ">" } else { " " },
                        app.choice_letter(i),
                        crate::item_phrase(&item.name, item.quantity)
                    ),
                    if i == app.selected { GOLD } else { TEXT },
                    2,
                    50,
                );
            }
        }
        if app.inventory_open {
            self.screen("INVENTORY", "ESC or i closes");
            if let Some(state) = &app.state {
                let o = &state.state().observation;
                let entries = crate::inventory_entries(app, o);
                if entries.is_empty() {
                    self.text(72, 156, "You are not carrying anything.", TEXT, 2, 60);
                }
                for (i, entry) in entries.iter().take(26).enumerate() {
                    self.text(
                        72,
                        156 + i * 22,
                        &entry.long(),
                        if entry.equipped.is_some() {
                            ACCENT
                        } else {
                            TEXT
                        },
                        2,
                        64,
                    );
                }
            }
        }
        if let Some(scroll) = app.message_log {
            self.screen(
                "MESSAGES",
                "UP/DOWN scroll   PGUP/PGDN page   ESC closes   (newest at the bottom)",
            );
            let rows = app.messages.history_rows(crate::messages::WIDTH);
            let end = rows.len().saturating_sub(scroll);
            let start = end.saturating_sub(crate::MESSAGE_LOG_ROWS);
            if rows.is_empty() {
                self.text(72, 156, "No messages yet.", MUTED, 2, 60);
            }
            for (i, row) in rows[start..end].iter().enumerate() {
                self.text(72, 152 + i * 22, row, TEXT, 2, crate::messages::WIDTH);
            }
        }
        if app.help_open {
            self.screen("HELP", "ESC or ? closes");
            for (i, line) in crate::HELP.iter().enumerate() {
                self.text(72, 150 + i * 19, line, TEXT, 2, 66);
            }
        }
        if app.places_open {
            self.screen(
                "REMEMBERED PLACES",
                "UP/DOWN select   ENTER rename   ESC close. Names are personal mnemonics.",
            );
            if let Some(state) = &app.state {
                let observation = &state.state().observation;
                let start = app.place_selected.saturating_sub(12);
                for (i, place) in observation.places.iter().enumerate().skip(start).take(15) {
                    let visible = observation
                        .visible_cells
                        .iter()
                        .any(|c| c.key == place.key && !c.wall);
                    self.text(
                        72,
                        156 + (i - start) * 26,
                        &format!(
                            "{} {} ({})",
                            if i == app.place_selected { ">" } else { " " },
                            place.name,
                            if visible { "in sight" } else { "remembered" }
                        ),
                        if i == app.place_selected { GOLD } else { TEXT },
                        2,
                        66,
                    );
                }
                if observation.places.is_empty() {
                    self.text(72, 156, "No places discovered yet.", TEXT, 2, 66);
                }
            }
            if let Some(name) = &app.place_name {
                self.text(
                    72,
                    625,
                    "New name (ENTER saves, ESC cancels):",
                    ACCENT,
                    2,
                    66,
                );
                for (i, line) in crate::wrap(name, 66).iter().take(2).enumerate() {
                    self.text(72, 655 + i * 26, line, TEXT, 2, 66);
                }
            }
        }
        if let Some(page) = &app.history_page {
            self.screen(
                "HISTORY",
                "UP/DOWN scroll  PGUP older page  PGDN live view  ESC close",
            );
            for (i, line) in history_lines(&page.entries)
                .iter()
                .skip(app.history_scroll)
                .take(crate::HISTORY_ROWS)
                .enumerate()
            {
                self.text(72, 152 + i * 24, line, TEXT, 2, 66);
            }
            if page.entries.is_empty() {
                self.text(72, 156, "No history entries yet.", MUTED, 2, 60);
            }
        }
    }
}

/// Convert native mouse pixels through the window's aspect-ratio letterboxing.
pub fn logical_mouse(x: f32, y: f32, width: usize, height: usize) -> Option<(usize, usize)> {
    let scale = (width as f32 / WIDTH as f32).min(height as f32 / HEIGHT as f32);
    if scale <= 0.0 {
        return None;
    }
    let x = (x - (width as f32 - WIDTH as f32 * scale) / 2.0) / scale;
    let y = (y - (height as f32 - HEIGHT as f32 * scale) / 2.0) / scale;
    (x >= 0.0 && y >= 0.0 && x < WIDTH as f32 && y < HEIGHT as f32)
        .then_some((x as usize, y as usize))
}
