//! Fixed logical canvas, scaled by the native window without changing game state.
use crate::{glyph_at, history_lines, history_text, App};
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
        self.text(28, 54, "ASCII / TWO-ROOM EXPEDITION", MUTED, 1, 60);
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
            self.text(44, 110, &o.region.name, TEXT, 2, 40);
            self.text(
                44,
                140,
                &format!(
                    "POSITION ({}, {}, {})   TICK {}   REV {}",
                    o.position.x,
                    o.position.y,
                    o.position.z,
                    o.tick,
                    state.state().revision
                ),
                MUTED,
                1,
                84,
            );
            // A bounded viewport prevents large disclosed rooms allocating large
            // buffers; center on the actor when a future room exceeds the view.
            let cols = o.region.width.clamp(1, 11);
            let rows = o.region.depth.clamp(1, 5);
            let x0 = (o.position.x - cols / 2).clamp(0, (o.region.width - cols).max(0));
            let y0 = (o.position.y - rows / 2).clamp(0, (o.region.depth - rows).max(0));
            let left = 396 - cols as usize * 28;
            let top = 302 - rows as usize * 28;
            for row in 0..rows {
                for col in 0..cols {
                    let x = left + col as usize * 56;
                    let y = top + row as usize * 56;
                    let glyph = glyph_at(o, x0 + col, y0 + row);
                    self.rect(x, y, 52, 52, if glyph == '@' { 0x203f41 } else { 0x192733 });
                    let color = match glyph {
                        '@' => ACCENT,
                        '!' => GOLD,
                        '+' => 0x8cbafa,
                        '&' => 0xef958c,
                        _ => 0x546c7e,
                    };
                    self.text(x + 14, y + 14, &glyph.to_string(), color, 3, 1);
                }
            }
            self.text(
                44,
                468,
                "@ YOU   ! ITEM   + PASSAGE   & ACTOR   . FLOOR",
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
            self.text(804, 280, "IN THIS ROOM", ACCENT, 2, 22);
            if o.ground_items.is_empty() {
                self.text(804, 316, "No items in sight.", MUTED, 2, 22);
            }
            for (i, item) in o.ground_items.iter().take(4).enumerate() {
                self.text(804, 316 + i * 36, &item.item.name, TEXT, 2, 22);
                self.text(
                    804,
                    337 + i * 36,
                    &format!(
                        "({}, {}, {}){}",
                        item.position.x,
                        item.position.y,
                        item.position.z,
                        if item.position == o.position
                            && app.role != tor_protocol::AccessRole::Spectator
                        {
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
            "ARROWS/HJKL move  U/D level  SPACE wait  G pickup  C control  R release  N note  F2 history  ESC quit"
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
