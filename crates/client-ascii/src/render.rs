//! Fixed logical canvas, scaled by the native window without changing game state.
use crate::{history_lines, App};
use font8x8::{UnicodeFonts, BASIC_FONTS};
use tor_protocol::Position;

pub const WIDTH: usize = 1200;
pub const HEIGHT: usize = 800;
const CELL: usize = 16;
const MAP_TOP: usize = 48;
const MAP_ROWS: usize = 45;
const MAP_COLS: usize = 75;
const BG: u32 = 0x0c1118;
const PANEL: u32 = 0x131d27;
const BORDER: u32 = 0x263747;
const TEXT: u32 = 0xd9e3e9;
const MUTED: u32 = 0x869ba9;
const ACCENT: u32 = 0x67d8bd;
const GOLD: u32 = 0xedc579;
pub const MEMORY_COLOR: u32 = tor_client_hack::REMEMBERED_COLOR;

/// The same tiles drive native painting and opt-in presentation diagnostics.
#[derive(serde::Serialize)]
pub struct MapTile {
    pub position: Position,
    pub glyph: char,
    pub remembered: bool,
    pub color: u32,
    pub center: (usize, usize),
}

pub fn map_tiles(app: &App) -> Vec<MapTile> {
    let Some(state) = &app.state else {
        return Vec::new();
    };
    let Some((origin_x, origin_y)) = app.map_origin() else {
        return Vec::new();
    };
    let observation = &state.state().observation;
    let chart: Vec<_> = state.map_memory().collect();
    let mut tiles = Vec::new();
    for column in tor_client_hack::map_columns(observation, &chart) {
        let Some(col) = column.x.checked_sub(origin_x) else {
            continue;
        };
        let Some(row) = column.y.checked_sub(origin_y) else {
            continue;
        };
        if !(0..MAP_COLS as i32).contains(&col) || !(0..MAP_ROWS as i32).contains(&row) {
            continue;
        }
        let col = col as usize;
        let row = row as usize;
        tiles.push(MapTile {
            position: Position {
                x: column.x,
                y: column.y,
                z: observation.position.z,
            },
            glyph: column.glyph.ch,
            remembered: column.glyph.remembered,
            color: column.glyph.color,
            center: (col * CELL + CELL / 2, MAP_TOP + row * CELL + CELL / 2),
        });
    }
    tiles
}

/// World cell under a map pixel, using the stored viewport. `None` off the map.
pub fn cell_at(app: &App, x: usize, y: usize) -> Option<Position> {
    if !(MAP_TOP..MAP_TOP + MAP_ROWS * CELL).contains(&y) || x >= WIDTH {
        return None;
    }
    let (origin_x, origin_y) = app.map_origin()?;
    let col = (x / CELL) as i32;
    let row = ((y - MAP_TOP) / CELL) as i32;
    if !(0..MAP_COLS as i32).contains(&col) || !(0..MAP_ROWS as i32).contains(&row) {
        return None;
    }
    let z = app.state.as_ref()?.state().observation.position.z;
    Some(Position {
        x: origin_x.checked_add(col)?,
        y: origin_y.checked_add(row)?,
        z,
    })
}

pub fn cell_center(app: &App, position: Position) -> Option<(usize, usize)> {
    let z0 = app.state.as_ref()?.state().observation.position.z;
    if position.z != z0 {
        return None;
    }
    map_tiles(app)
        .into_iter()
        .find(|tile| tile.position.x == position.x && tile.position.y == position.y)
        .map(|tile| tile.center)
}

pub fn status_lines(app: &App) -> [String; 2] {
    let mut line1 = Vec::new();
    if let Some(state) = &app.state {
        let observation = &state.state().observation;
        if let Some(combat) = &observation.combat {
            line1.push(format!("HP {}/{}", combat.hp, combat.max_hp));
            if combat.dead || combat.hp == 0 {
                line1.push("Dead".into());
            } else if u64::from(combat.hp) * 4 <= u64::from(combat.max_hp) {
                line1.push("Badly wounded".into());
            } else if u64::from(combat.hp) * 2 <= u64::from(combat.max_hp) {
                line1.push("Wounded".into());
            }
        }
        line1.push(format!("T:{}", observation.tick));
        if let Some(combat) = &observation.combat {
            if let Some(remaining) = combat.preparation_remaining {
                let mut prep = format!("Prep {remaining}");
                if !combat.preparation_active {
                    prep.push_str(" interrupted");
                }
                line1.push(prep);
            } else if combat.recovery_remaining > 0 {
                line1.push(format!("Recover {}", combat.recovery_remaining));
            }
        }
        line1.push(
            if observation.ready {
                "Ready"
            } else {
                "Waiting"
            }
            .into(),
        );
        if state.state().wizard_game {
            line1.push("WIZARD GAME".into());
        }
    }
    line1.push(
        if !app.connected {
            "DISCONNECTED"
        } else if app.role == tor_protocol::AccessRole::Spectator {
            "SPECTATOR"
        } else if app.state.as_ref().is_some_and(|state| state.has_control()) {
            "IN CONTROL"
        } else {
            "OBSERVING"
        }
        .into(),
    );
    let mut line2 = Vec::new();
    if let Some(enclosure) = enclosure_label(app) {
        line2.push(enclosure);
    }
    if let Some(travel) = app.state.as_ref().and_then(|state| state.travel()) {
        line2.push(format!(
            "{} / {} STEPS",
            travel_label(travel.phase),
            travel.completed_steps
        ));
    }
    if !app.config.autopickup {
        line2.push("No pickup".into());
    }
    [line1.join("  "), line2.join("  ")]
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
    pub fn draw(&mut self, app: &App) {
        self.pixels.fill(BG);
        for (index, line) in app.message_lines().iter().enumerate() {
            self.text(
                0,
                index * CELL,
                line,
                if line == tor_client_hack::MORE {
                    ACCENT
                } else {
                    TEXT
                },
                2,
                MAP_COLS,
            );
        }
        for tile in map_tiles(app) {
            let selected = app.travel_cursor.is_some_and(|cursor| {
                cursor.x == tile.position.x
                    && cursor.y == tile.position.y
                    && cursor.z == tile.position.z
            });
            let x = tile.center.0 - CELL / 2;
            let y = tile.center.1 - CELL / 2;
            if selected {
                self.rect(x, y, CELL, CELL, GOLD);
            }
            self.text(
                x,
                y,
                &tile.glyph.to_string(),
                if selected { BG } else { tile.color },
                2,
                1,
            );
        }
        let lines = status_lines(app);
        self.text(0, 768, &lines[0], TEXT, 2, MAP_COLS);
        self.text(0, 784, &lines[1], TEXT, 2, MAP_COLS);
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
            self.text(188, 250, "Arrows or HJKL/YUBN", TEXT, 2, 50);
            self.text(188, 292, "ESC cancels without taking a turn", MUTED, 1, 80);
        }
        if !app.pickup.is_empty() {
            self.panel(160, 176, 880, 428);
            self.text(188, 204, "CHOOSE AN ITEM", ACCENT, 2, 50);
            self.text(
                188,
                239,
                &format!(
                    "UP/DOWN select  ENTER {}  Count: {}  ESC cancel",
                    if app.dropping { "drop" } else { "take" },
                    if app.quantity.is_empty() {
                        "all"
                    } else {
                        &app.quantity
                    }
                ),
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
                        format_args!("{} x {}", item.quantity, item.name)
                    ),
                    if i == app.selected { GOLD } else { TEXT },
                    2,
                    50,
                );
            }
        }
        if app.places_open {
            self.panel(48, 88, 1104, 632);
            self.text(72, 112, "REMEMBERED PLACES", ACCENT, 2, 65);
            self.text(
                72,
                144,
                "UP/DOWN select   ENTER rename   ESC close. Names are personal mnemonics.",
                MUTED,
                1,
                120,
            );
            if let Some(state) = &app.state {
                let observation = &state.state().observation;
                let start = app.place_selected.saturating_sub(12);
                for (i, place) in observation.places.iter().enumerate().skip(start).take(15) {
                    let visible = observation
                        .visible_cells
                        .iter()
                        .any(|cell| cell.key == place.key && !cell.wall);
                    self.text(
                        72,
                        178 + (i - start) * 26,
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
                    self.text(72, 178, "No places discovered yet.", TEXT, 2, 66);
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
        if app.scrollback_open() {
            self.panel(48, 88, 1104, 632);
            self.text(72, 112, "EARLIER MESSAGES", ACCENT, 2, 65);
            self.text(72, 144, "ESC close", MUTED, 1, 40);
            let lines = app.scrollback_lines();
            if lines.is_empty() {
                self.text(72, 178, "No earlier messages.", MUTED, 2, 60);
            } else {
                let start = lines.len().saturating_sub(20);
                for (i, line) in lines.iter().skip(start).enumerate() {
                    self.text(72, 178 + i * 16, line, TEXT, 1, MAP_COLS);
                }
            }
        }
    }
}

/// Floor and ceiling at `observation.position`. Both missing omits the clause.
fn enclosure_label(app: &App) -> Option<String> {
    use tor_client_common::surfaces;
    let state = app.state.as_ref()?;
    let observation = &state.state().observation;
    let floor = surfaces::floor_below(&observation.visible_cells, observation.position);
    let ceiling = surfaces::ceiling_above(&observation.visible_cells, observation.position);
    if floor.is_none() && ceiling.is_none() {
        return None;
    }
    Some(format!(
        "FLOOR: {} / CEILING: {}",
        floor.map_or("not visible", surfaces::material),
        ceiling.map_or_else(
            || "not visible".into(),
            |(cell, distance)| format!(
                "{} ({} ft above feet)",
                surfaces::material(cell),
                u64::from(distance) * 5
            )
        )
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_characters_are_in_the_bitmap_font() {
        for ch in ['#', '.', '+', '/', '<', '>', '!', '&', '^', '@', 'r', '$'] {
            assert!(BASIC_FONTS.get(ch).is_some(), "{ch}");
        }
    }
}
