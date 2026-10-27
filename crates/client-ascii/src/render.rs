//! Fixed logical canvas, scaled by the native window without changing game state.
//!
//! Layout, as in NetHack: message and prompt lines across the top, the map
//! across the full width, three status rows at the bottom. Everything else
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
const HINT: &str = "? help  hjklyubn move/run  < > stairs  _ travel  ; look  i inventory  m charts  @ stats  z abilities  g pick up  a attack  ^P messages";

/// The map as drawn: one tile per column, current and remembered.
pub fn map_tiles(
    state: &tor_client_common::ClientState,
    palette: &tor_client_common::Palette,
) -> Vec<MapTile> {
    let current: std::collections::BTreeSet<_> = state
        .state()
        .observation
        .visible_cells
        .iter()
        .map(|c| (c.position.x, c.position.y, c.position.z))
        .collect();
    let current_appearances = map::appearance_cells(&state.state().observation);
    let cells = prepare_appearances(state.map_memory(), &current_appearances);
    map::appearance_tiles(&state.state().observation, &cells, palette, &|position| {
        current.contains(&(position.x, position.y, position.z))
    })
}

fn prepare_appearances<'a>(
    memory: impl Iterator<Item = tor_client_common::MapCell<'a>>,
    current: &'a [tor_protocol::RememberedMapCell],
) -> Vec<map::PositionedAppearance<'a>> {
    let mut cells: std::collections::BTreeMap<_, _> = memory
        .map(|cell| {
            (
                cell.position,
                map::PositionedAppearance {
                    position: cell.position,
                    appearance: &cell.cell.appearance,
                },
            )
        })
        .collect();
    for cell in current {
        cells.insert(
            cell.position,
            map::PositionedAppearance {
                position: cell.position,
                appearance: cell,
            },
        );
    }
    cells.into_values().collect()
}

/// The known cell under a pixel, current or remembered.
pub fn known_cell_at(
    state: &tor_client_common::ClientState,
    x: usize,
    y: usize,
) -> Option<tor_protocol::Position> {
    let (dx, dy) = map::column_at(x, y)?;
    let feet = state.state().observation.position;
    let position = tor_protocol::Position {
        x: feet.x.checked_add(dx)?,
        y: feet.y.checked_add(dy)?,
        z: feet.z,
    };
    state.map_cell(position).map(|c| c.position)
}

/// The known cell a column selects, current or remembered.
pub fn known_column(
    state: &tor_client_common::ClientState,
    x: i32,
    y: i32,
) -> Option<tor_protocol::Position> {
    let position = tor_protocol::Position {
        x,
        y,
        z: state.state().observation.position.z,
    };
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
        for tile in map_tiles(state, &app.palette) {
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

    /// Three status rows, as in NetHack, and a one-line key hint.
    fn draw_status(&mut self, app: &App) {
        if let Some(state) = &app.state {
            // Preserve readable 16-pixel glyphs. Lower-priority whole fields
            // yield to health, warnings, gold and resources rather than shrinking.
            let scale = 2;
            let columns = (WIDTH.saturating_sub(48)) / (8 * scale);
            for (row, line) in crate::status_lines_at_width(app, state, columns)
                .iter()
                .enumerate()
            {
                self.text(24, 696 + row * 20, line, TEXT, scale, columns);
            }
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
        if app.charts_open {
            self.screen(
                "DETECTION CHARTS — READ ONLY",
                "UP/DOWN choose or scroll   ENTER load   TAB catalog   PgUp/PgDn rows   ESC closes",
            );
            for (i, line) in app
                .chart_lines()
                .iter()
                .skip(app.chart_scroll)
                .take(24)
                .enumerate()
            {
                self.text(72, 156 + i * 22, line, TEXT, 1, 96);
            }
            return;
        }
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
        if let Some(draft) = &app.wizard_command {
            self.panel(60, 180, 1080, 424);
            self.text(84, 206, "WIZARD COMMAND", ACCENT, 2, 60);
            self.text(84, 240, "ENTER send   ESC cancel", MUTED, 1, 80);
            self.text(
                84,
                260,
                "creature inspect <actor>   arena pause|resume|step",
                MUTED,
                1,
                80,
            );
            let lines = crate::wrap(draft, 64);
            let start = lines.len().saturating_sub(10);
            for (i, line) in lines.iter().skip(start).enumerate() {
                self.text(84, 290 + i * 26, line, TEXT, 2, 64);
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
        if app.zap_item().is_some() {
            self.panel(160, 176, 880, 160);
            self.text(188, 204, "ZAP IN WHICH DIRECTION?", ACCENT, 2, 50);
            self.text(188, 250, "hjklyubn, arrows, or < >", TEXT, 2, 50);
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
            for (i, row) in app.item_rows().iter().enumerate().skip(start).take(11) {
                self.text(
                    188,
                    240 + (i - start) * 32,
                    row,
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
                    // Preserve equipped/name styling while the glyph shares
                    // its asset colour with the map.
                    self.text(
                        72 + 4 * 16,
                        156 + i * 22,
                        &entry.glyph.to_string(),
                        entry.color,
                        2,
                        1,
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
        if app.stats_open {
            self.screen(
                app.stats_title(),
                "UP/DOWN scroll   PGUP/PGDN page   ESC or @ closes",
            );
            let rows = app.stats_rows();
            let start = app
                .stats_scroll
                .min(rows.len().saturating_sub(crate::STATS_ROWS));
            for (i, row) in rows.iter().skip(start).take(crate::STATS_ROWS).enumerate() {
                self.text(72, 152 + i * 22, row, TEXT, 2, crate::STATS_WIDTH);
            }
        }
        if !app.ability_choices.is_empty() {
            self.screen(
                "ABILITIES",
                if app.weapon_selection_available() {
                    "UP/DOWN selects   TAB weapon   ENTER target   ESC cancels"
                } else {
                    "UP/DOWN selects   ENTER target   ESC cancels"
                },
            );
            for (i, ability) in app.ability_choices.iter().enumerate() {
                self.text(
                    72,
                    156 + i * 26,
                    &format!(
                        "{} {}",
                        if i == app.selected { ">" } else { " " },
                        tor_client_common::abilities::ability_label(ability)
                    ),
                    if i == app.selected { GOLD } else { TEXT },
                    2,
                    65,
                );
            }
        }
        if !app.hold_choices.is_empty() {
            self.screen(
                "ESCAPE A HOLD",
                "UP/DOWN selects   ENTER escapes   ESC or F11 cancels",
            );
            let start = app.selected.saturating_sub(crate::STATS_ROWS - 1);
            for (index, choice) in app
                .hold_choices
                .iter()
                .enumerate()
                .skip(start)
                .take(crate::STATS_ROWS)
            {
                self.text(
                    72,
                    156 + (index - start) * 22,
                    &format!(
                        "{} {}. {}",
                        if index == app.selected { ">" } else { " " },
                        index + 1,
                        choice.label
                    ),
                    if index == app.selected { GOLD } else { TEXT },
                    2,
                    crate::STATS_WIDTH,
                );
            }
        }
        if app.advancement_open {
            self.screen(
                "ADVANCEMENT",
                "UP/DOWN scroll   PGUP/PGDN previous/next   ESC closes",
            );
            for (i, row) in app
                .advancement_rows()
                .iter()
                .skip(app.advancement_scroll)
                .take(crate::ADVANCEMENT_ROWS)
                .enumerate()
            {
                self.text(72, 152 + i * 22, row, TEXT, 2, crate::STATS_WIDTH);
            }
            for (i, row) in tor_client_common::advancement::HELP.lines().enumerate() {
                self.text(72, 518 + i * 19, row, MUTED, 1, 105);
            }
            let tail: String = app
                .advancement_command
                .chars()
                .rev()
                .take(63)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            self.text(72, 668, &format!("> {tail}"), GOLD, 2, 65);
        }
        if app.supply_selection.is_some() {
            self.screen(
                "SUPPLIES",
                "UP/DOWN selects   SPACE toggles exact items   ENTER uses   ESC cancels",
            );
            let rows = app.supply_rows();
            let start = app.selected.saturating_sub(12);
            for (index, row) in rows.iter().skip(start).take(15).enumerate() {
                self.text(72, 152 + index * 26, row, TEXT, 2, 70);
            }
        }
        if let Some(menu) = &app.projectile_selection {
            self.screen(
                &menu.title(),
                if menu.step() == tor_client_common::projectiles::Step::Direction {
                    "Movement keys choose direction   ESC cancels"
                } else {
                    "UP/DOWN selects   ENTER chooses   LEFT/RIGHT pages   ESC cancels"
                },
            );
            let rows = app.projectile_rows();
            let start = app.selected.saturating_sub(12);
            for (index, row) in rows.iter().skip(start).take(18).enumerate() {
                self.text(72, 152 + index * 26, row, TEXT, 2, 70);
            }
        }
        if app.weapon_confirmation.is_some() {
            self.screen(
                "EXACT WEAPON ACTION PREVIEW",
                "ENTER commits after reply   ESC cancels",
            );
            let mut line = 0;
            for row in app.weapon_preview_rows() {
                for chunk in row.chars().collect::<Vec<_>>().chunks(65) {
                    self.text(
                        72,
                        152 + line * 24,
                        &chunk.iter().collect::<String>(),
                        TEXT,
                        2,
                        65,
                    );
                    line += 1;
                }
            }
        }
        if app.weapon_selection.is_some() {
            self.screen(
                "WEAPON FOR THIS ACTION",
                "UP/DOWN selects   ENTER chooses   ESC keeps previous choice",
            );
            let rows = app.weapon_rows();
            let start = app.selected.saturating_sub(12);
            for (index, row) in rows.iter().skip(start).take(15).enumerate() {
                self.text(72, 152 + index * 26, row, TEXT, 2, 70);
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
        if let Some(choice) = app.identification_choice() {
            self.screen(
                &format!(
                    "IDENTIFY: {}",
                    choice
                        .classes
                        .iter()
                        .map(|class| format!("{class:?}"))
                        .collect::<Vec<_>>()
                        .join(" / ")
                )
                .to_uppercase(),
                "letter or UP/DOWN + ENTER chooses   ESC aborts the paid choice",
            );
            let rows = app.identification_rows();
            if rows.is_empty() {
                self.text(
                    72,
                    156,
                    "No items offered in this category. Esc aborts.",
                    TEXT,
                    2,
                    65,
                );
            }
            let start = app
                .identification_selected
                .saturating_sub(crate::STATS_ROWS - 1);
            for (index, row) in rows.iter().enumerate().skip(start).take(crate::STATS_ROWS) {
                self.text(
                    72,
                    156 + (index - start) * 22,
                    row,
                    if index == app.identification_selected {
                        GOLD
                    } else {
                        TEXT
                    },
                    2,
                    crate::STATS_WIDTH,
                );
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

#[cfg(test)]
mod borrowed_map_tests {
    use super::*;
    use tor_client_common::{MapCell, RememberedCell};
    use tor_protocol::{ItemClass, MapItemAppearance, Position, RememberedMapCell};

    fn position(x: i32, z: i32) -> Position {
        Position { x, y: 0, z }
    }
    fn cell(x: i32, z: i32) -> RememberedMapCell {
        RememberedMapCell {
            object_presentation: None,
            key: format!("cell-{x}-{z}"),
            position: position(x, z),
            wall: false,
            material: "retained stone description".into(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            door: None,
            trap: None,
            ground_items: vec![],
        }
    }
    fn record(appearance: RememberedMapCell) -> RememberedCell {
        RememberedCell {
            appearance,
            last_seen_tick: Some(1),
            last_seen_revision: Some(1),
        }
    }
    fn pile(class: ItemClass) -> MapItemAppearance {
        MapItemAppearance {
            presentation: None,
            class,
            name: "remembered object".into(),
            description: "long retained description".into(),
            appearance: "remembered object".into(),
            identified: false,
            quantity: 1,
            asset: None,
        }
    }

    #[test]
    fn retained_map_occurrences_borrow_original_payload_storage() {
        let mut appearance = cell(90, 7);
        appearance.ground_items.push(pile(ItemClass::Potion));
        appearance.ground_items[0].presentation = Some(tor_protocol::ObjectPresentation {
            name: "retained false stairs".into(),
            asset: None,
            kind: tor_protocol::ObjectPresentationKind::Stairs,
        });
        appearance.stairs_down = true;
        appearance.ground_items[0].name = "object".into();
        appearance.ground_items[0].appearance = "object".into();
        appearance.ground_items[0].description.clear();
        appearance.object_presentation = Some(tor_protocol::ObjectPresentation {
            name: "retained false potion".into(),
            asset: None,
            kind: tor_protocol::ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        });
        let saved = record(appearance);
        let prepared = prepare_appearances(
            [
                MapCell {
                    position: position(2, 0),
                    cell: &saved,
                },
                MapCell {
                    position: position(4, 1),
                    cell: &saved,
                },
            ]
            .into_iter(),
            &[],
        );
        assert_eq!(
            prepared.iter().map(|row| row.position).collect::<Vec<_>>(),
            [position(2, 0), position(4, 1)]
        );
        for row in &prepared {
            assert!(std::ptr::eq(row.appearance, &saved.appearance));
            assert_eq!(row.material.as_ptr(), saved.material.as_ptr());
            assert_eq!(row.ground_items.as_ptr(), saved.ground_items.as_ptr());
            assert_eq!(
                row.object_presentation.as_ref().unwrap().name.as_ptr(),
                saved.object_presentation.as_ref().unwrap().name.as_ptr()
            );
            assert_eq!(
                row.ground_items[0]
                    .presentation
                    .as_ref()
                    .unwrap()
                    .name
                    .as_ptr(),
                saved.ground_items[0]
                    .presentation
                    .as_ref()
                    .unwrap()
                    .name
                    .as_ptr()
            );
        }
    }

    #[test]
    fn positioned_merge_preserves_materialized_tiles_heights_order_and_current_overlays() {
        let mut o = tor_client_common::test_support::observation();
        o.visible_actors.clear();
        let mut lower = cell(90, 7);
        lower.ground_items.push(pile(ItemClass::Potion));
        let mut stairs = cell(91, 7);
        stairs.stairs_up = true;
        let mut wall = cell(92, 7);
        wall.wall = true;
        let saved = [
            record(lower),
            record(stairs),
            record(wall),
            record(cell(93, 7)),
        ];
        let memory = [
            MapCell {
                position: position(0, 0),
                cell: &saved[2],
            },
            MapCell {
                position: position(1, -1),
                cell: &saved[0],
            },
            MapCell {
                position: position(2, 0),
                cell: &saved[1],
            },
            MapCell {
                position: position(3, 0),
                cell: &saved[2],
            },
            MapCell {
                position: position(3, 1),
                cell: &saved[3],
            },
            MapCell {
                position: position(4, 1),
                cell: &saved[1],
            },
        ];
        let mut current_item = cell(1, 0);
        current_item.ground_items.push(pile(ItemClass::Wand));
        let current = vec![cell(0, 0), current_item];
        let prepared = prepare_appearances(memory.into_iter(), &current);
        assert!(
            !prepared[0].wall,
            "current disclosure replaces the stale wall at the same occurrence"
        );
        // Independent old materialized merge; each retained occurrence gets its own position.
        let mut old: std::collections::BTreeMap<_, _> = memory
            .into_iter()
            .map(|row| (row.position, row.appearance()))
            .collect();
        for row in &current {
            old.insert(row.position, row.clone());
        }
        let old_materialized: Vec<_> = old.into_values().collect();
        let old: Vec<_> = old_materialized
            .iter()
            .map(|row| map::PositionedAppearance {
                position: row.position,
                appearance: row,
            })
            .collect();
        let is_current = |p| current.iter().any(|row| row.position == p);
        let expected = map::appearance_tiles(&o, &old, &Default::default(), &is_current);
        let actual = map::appearance_tiles(&o, &prepared, &Default::default(), &is_current);
        assert_eq!(actual, expected);
        assert_eq!(
            actual
                .iter()
                .map(|row| (row.position, row.kind, row.glyph, row.remembered))
                .collect::<Vec<_>>(),
            [
                (position(0, 0), Kind::Player, '@', false),
                (position(1, 0), Kind::Item, '!', false),
                (position(2, 0), Kind::StairsUp, '<', true),
                (position(3, 0), Kind::LowWall, '#', true),
                (position(4, 1), Kind::Floor, '.', true),
            ]
        );
        // Current disclosed piles precede even lower retained planes, then creatures win.
        o.ground_items.push(tor_protocol::GroundItemView {
            presentation: None,
            item: tor_protocol::ItemView {
                id: tor_protocol::ItemTarget::from_digest([44; 32]),
                name: "wand".into(),
                class: ItemClass::Wand,
                appearance: "wand".into(),
                identified: false,
                quantity: 1,
                description: String::new(),
                corpse: None,
                asset: None,
                known_currency: None,
                known_supply: None,
            },
            position: position(1, 0),
            reachable: false,
            supply_reachable: false,
        });
        let overlaid = map::appearance_tiles(&o, &prepared, &Default::default(), &is_current);
        assert_eq!(overlaid[1].glyph, '/');
        o.visible_actors.push(tor_protocol::ActorView {
            id: tor_protocol::ActorTarget::from_digest([45; 32]),
            name: "disclosed creature".into(),
            description: String::new(),
            asset: None,
            appearance_seed: None,
            position: position(1, 0),
        });
        let occupied = map::appearance_tiles(&o, &prepared, &Default::default(), &is_current);
        assert_eq!(occupied[1].kind, Kind::Creature);
        assert_eq!(
            occupied,
            map::appearance_tiles(&o, &old, &Default::default(), &is_current)
        );
    }
}
