//! Fixed logical canvas, scaled by the native window without changing game state.
use crate::{glyph_at_level, history_lines, history_text, App};
use font8x8::{UnicodeFonts, BASIC_FONTS};

pub const WIDTH: usize = 1200;
pub const HEIGHT: usize = 800;
const BG: u32 = 0x0c1118;
const PANEL: u32 = 0x131d27;
const BORDER: u32 = 0x263747;
const TEXT: u32 = 0xd9e3e9;
const MUTED: u32 = 0x869ba9;
const ACCENT: u32 = 0x67d8bd;
const GOLD: u32 = 0xedc579;

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
    fn text(&mut self, x: usize, y: usize, text: &str, color: u32, scale: usize, limit: usize) {
        let clipped = text.chars().count() > limit;
        for (i, ch) in text.chars().take(limit).enumerate() {
            let ch = if clipped && i + 1 == limit { '~' } else { ch };
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
    pub fn draw(&mut self, app: &App) {
        self.pixels.fill(BG);
        self.text(28, 24, "THRESHOLDS OF RUIN", TEXT, 2, 35);
        self.text(28, 54, "ASCII / EXPEDITION", MUTED, 1, 60);
        if app.state.as_ref().is_some_and(|s| s.state().wizard_game) {
            self.text(560, 30, "WIZARD GAME", GOLD, 2, 20);
        }
        let control = if !app.connected {
            "DISCONNECTED"
        } else if app.role == tor_protocol::AccessRole::Spectator {
            "SPECTATOR"
        } else if app.state.as_ref().is_some_and(|s| s.has_control()) {
            "IN CONTROL"
        } else {
            "OBSERVING"
        };
        self.text(
            936,
            30,
            control,
            if app.connected { ACCENT } else { GOLD },
            2,
            16,
        );
        self.panel(24, 88, 744, 408);
        self.panel(784, 88, 392, 408);
        self.panel(24, 512, 1152, 208);
        self.text(44, 532, "RECENT HISTORY", MUTED, 1, 70);
        if let Some(state) = &app.state {
            let o = &state.state().observation;
            self.text(44, 110, "YOUR SURROUNDINGS", TEXT, 2, 40);
            self.text(44, 140, &format!("TICK {}", o.tick), MUTED, 1, 84);
            for panel in map_panels(o) {
                if panel.label {
                    self.text(
                        panel.left,
                        panel.top - 14,
                        &format!("Z {:+}", panel.z),
                        MUTED,
                        1,
                        18,
                    );
                }
                for row in 0..panel.rows {
                    for col in 0..panel.cols {
                        let position = tor_protocol::Position {
                            x: panel.x0 + col as i32,
                            y: panel.y0 + row as i32,
                            z: panel.z,
                        };
                        let glyph = glyph_at_level(o, position.x, position.y, position.z);
                        let selected = app.travel_cursor == Some(position);
                        if glyph == ' ' && !selected {
                            continue;
                        }
                        let x = panel.left + col * panel.step;
                        let y = panel.top + row * panel.step;
                        self.rect(
                            x,
                            y,
                            panel.step - 1,
                            panel.step - 1,
                            if selected {
                                GOLD
                            } else if glyph == '@' {
                                0x203f41
                            } else {
                                0x192733
                            },
                        );
                        let color = if selected {
                            BG
                        } else {
                            match glyph {
                                '@' => ACCENT,
                                '!' => GOLD,
                                '<' | '>' => 0x8cbafa,
                                '&' => 0xef958c,
                                _ => 0x7890a2,
                            }
                        };
                        let scale = (panel.step / 12).clamp(1, 3);
                        let pad = (panel.step - 8 * scale) / 2;
                        self.text(x + pad, y + pad, &glyph.to_string(), color, scale, 1);
                    }
                }
            }
            if let Some(travel) = state.travel() {
                self.text(
                    220,
                    140,
                    &format!(
                        "TRAVEL: {} / {} STEPS{}",
                        travel_label(travel.phase),
                        travel.completed_steps,
                        if travel.phase == tor_protocol::TravelPhase::Active {
                            " / ESC CANCEL"
                        } else {
                            ""
                        }
                    ),
                    GOLD,
                    1,
                    64,
                );
            }
            self.text(
                44,
                468,
                "@ YOU   ! ITEM   & ACTOR   # WALL   . FLOOR   < > STAIRS",
                MUTED,
                1,
                84,
            );
            self.text(804, 110, "INVENTORY", ACCENT, 2, 22);
            if o.inventory.is_empty() {
                self.text(804, 148, "Nothing carried yet.", MUTED, 2, 22);
            }
            for (i, item) in o.inventory.iter().take(5).enumerate() {
                self.text(804, 146 + i * 22, &format!("! {}", item.name), TEXT, 2, 22);
            }
            if o.inventory.len() > 5 {
                self.text(
                    804,
                    257,
                    &format!("... {} more", o.inventory.len() - 5),
                    MUTED,
                    1,
                    40,
                );
            }
            self.text(804, 280, "IN SIGHT", ACCENT, 2, 22);
            if o.ground_items.is_empty() {
                self.text(804, 316, "No items in sight.", MUTED, 2, 22);
            }
            for (i, item) in o.ground_items.iter().take(4).enumerate() {
                self.text(804, 316 + i * 36, &item.item.name, TEXT, 2, 22);
                self.text(
                    804,
                    337 + i * 36,
                    &format!(
                        "OFFSET ({}, {}, {}){}",
                        item.position.x,
                        item.position.y,
                        item.position.z,
                        if item.reachable && app.role != tor_protocol::AccessRole::Spectator {
                            "  [G] PICK UP"
                        } else {
                            ""
                        }
                    ),
                    MUTED,
                    1,
                    44,
                );
            }
            let entries = state.history();
            for (i, entry) in entries
                .iter()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .iter()
                .rev()
                .enumerate()
            {
                self.text(
                    44,
                    556 + i * 24,
                    &format!("{:>5}  {}", entry.tick, history_text(entry)),
                    TEXT,
                    2,
                    68,
                );
            }
        } else {
            self.text(44, 190, "Connecting...", MUTED, 2, 40);
        }
        self.text(
            28,
            738,
            &app.status,
            if app.busy { GOLD } else { TEXT },
            1,
            142,
        );
        let help = if app.role == tor_protocol::AccessRole::Spectator {
            "READ-ONLY   F2 history   UP/DOWN scroll history   PAGE UP older history   ESC close/quit"
        } else {
            "ARROWS/HJKL move  U/D level  _/CLICK travel  G pickup  SPACE wait  C/R control  N note  F2 history  ESC cancel/quit"
        };
        self.text(28, 768, help, MUTED, 1, 142);
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
        if !app.pickup.is_empty() {
            self.panel(160, 176, 880, 428);
            self.text(188, 204, "CHOOSE AN ITEM", ACCENT, 2, 50);
            self.text(
                188,
                239,
                "UP/DOWN select   ENTER take   ESC cancel",
                MUTED,
                1,
                80,
            );
            let start = app.selected.saturating_sub(8);
            for (i, item) in app.pickup.iter().enumerate().skip(start).take(9) {
                self.text(
                    188,
                    272 + (i - start) * 32,
                    &format!(
                        "{} {}",
                        if i == app.selected { ">" } else { " " },
                        item.name
                    ),
                    if i == app.selected { GOLD } else { TEXT },
                    2,
                    50,
                );
            }
        }
        if let Some(page) = &app.history_page {
            self.panel(48, 88, 1104, 632);
            self.text(72, 112, "HISTORY", ACCENT, 2, 65);
            self.text(
                72,
                144,
                "UP/DOWN scroll  PGUP older page  PGDN live view  ESC close",
                MUTED,
                1,
                120,
            );
            for (i, line) in history_lines(&page.entries)
                .iter()
                .skip(app.history_scroll)
                .take(20)
                .enumerate()
            {
                self.text(72, 178 + i * 26, line, TEXT, 2, 66);
            }
            if page.entries.is_empty() {
                self.text(72, 180, "No history entries yet.", MUTED, 2, 60);
            }
        }
    }
}

struct MapPanel {
    z: i32,
    x0: i32,
    y0: i32,
    rows: usize,
    cols: usize,
    step: usize,
    left: usize,
    top: usize,
    label: bool,
}
fn map_panels(o: &tor_protocol::Observation) -> Vec<MapPanel> {
    let mut levels: Vec<_> = o.visible_cells.iter().map(|c| c.position.z).collect();
    levels.sort_unstable();
    levels.dedup();
    levels.sort_by_key(|z| (z.abs(), *z));
    let panels = levels.len().max(1);
    levels
        .into_iter()
        .enumerate()
        .map(|(index, z)| {
            let (panel_x, panel_y, panel_w, panel_h): (usize, usize, usize, usize) = if index == 0 {
                (44, 166, 704, if panels == 1 { 292 } else { 236 })
            } else {
                let width = 704 / (panels - 1);
                (44 + (index - 1) * width, 402, width, 56)
            };
            let cells: Vec<_> = o
                .visible_cells
                .iter()
                .filter(|c| c.position.z == z)
                .collect();
            let x0 = cells.iter().map(|c| c.position.x).min().unwrap_or(0);
            let x1 = cells.iter().map(|c| c.position.x).max().unwrap_or(0);
            let y0 = cells.iter().map(|c| c.position.y).min().unwrap_or(0);
            let y1 = cells.iter().map(|c| c.position.y).max().unwrap_or(0);
            let cols = (x1 - x0 + 1) as usize;
            let rows = (y1 - y0 + 1) as usize;
            let step = (panel_w / cols)
                .min(panel_h.saturating_sub(16) / rows)
                .clamp(8, 52);
            MapPanel {
                z,
                x0,
                y0,
                rows,
                cols,
                step,
                left: panel_x + (panel_w - cols * step) / 2,
                top: panel_y + 16,
                label: panels > 1,
            }
        })
        .collect()
}

/// Hit testing shares the exact layout used to draw cells, including stair panels.
pub fn cell_at(
    o: &tor_protocol::Observation,
    x: usize,
    y: usize,
) -> Option<tor_protocol::Position> {
    for panel in map_panels(o) {
        if x >= panel.left
            && y >= panel.top
            && x < panel.left + panel.cols * panel.step
            && y < panel.top + panel.rows * panel.step
        {
            let position = tor_protocol::Position {
                x: panel.x0 + ((x - panel.left) / panel.step) as i32,
                y: panel.y0 + ((y - panel.top) / panel.step) as i32,
                z: panel.z,
            };
            return o
                .visible_cells
                .iter()
                .find(|c| c.position == position)
                .map(|c| c.position);
        }
    }
    None
}

pub fn cell_center(
    o: &tor_protocol::Observation,
    position: tor_protocol::Position,
) -> Option<(usize, usize)> {
    if !o.visible_cells.iter().any(|c| c.position == position) {
        return None;
    }
    map_panels(o)
        .into_iter()
        .find(|p| p.z == position.z)
        .map(|p| {
            (
                p.left + (position.x - p.x0) as usize * p.step + p.step / 2,
                p.top + (position.y - p.y0) as usize * p.step + p.step / 2,
            )
        })
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

fn travel_label(phase: tor_protocol::TravelPhase) -> &'static str {
    use tor_protocol::TravelPhase::*;
    match phase {
        Active => "MOVING",
        Arrived => "ARRIVED",
        Cancelled => "CANCELLED",
        Blocked => "PATH BLOCKED",
        Hazard => "POTENTIAL HAZARD IN SIGHT",
        DecisionRequired => "ANOTHER ACTOR NEEDS INPUT",
        ControlLost => "CONTROL RELEASED",
        WorldChanged => "WORLD CHANGED",
        Failed => "COULD NOT SAVE OR MOVE",
    }
}
