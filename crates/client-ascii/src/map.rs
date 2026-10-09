//! One map for every height. Each column of cells around the player is drawn
//! as what stands at the player's own level there: a wall, a low wall, a
//! floor, a drop, a door, stairs, an item or a creature. Heights well above
//! or below the player (a stair's far landing, a distant ceiling) are left out.
use std::collections::BTreeMap;
use tor_protocol::{CellView, Observation, Position};

/// Map cells shown across and down; odd, so the player is centred.
pub const COLS: i32 = 63;
pub const ROWS: i32 = 33;
/// Pixels per map cell.
pub const STEP: usize = 18;
/// The map's top-left pixel; the map spans the window's width.
pub const LEFT: usize = 33;
pub const TOP: usize = 92;
/// Heights relative to the player's feet that a column draws from.
pub const BAND: std::ops::RangeInclusive<i32> = -2..=3;

pub const MEMORY_COLOR: u32 = 0x626262;
const TERRAIN: u32 = 0x7890a2;
const LOW_WALL: u32 = 0xa58f6c;
const DROP: u32 = 0x5f86c4;
const STAIRS: u32 = 0x8cbafa;
const DOOR: u32 = 0xc9a46a;
const ITEM: u32 = 0xedc579;
const SELF: u32 = 0x67d8bd;
/// Creature colours, chosen by name so one kind always looks the same.
const CREATURES: [u32; 6] = [0xef958c, 0xf2c14e, 0xd98cf0, 0x7fd4f0, 0xa6e07a, 0xf08c4e];

/// What a column shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Player,
    Creature,
    Item,
    Wall,
    /// Solid at the feet, open above: a low wall, ledge or step.
    LowWall,
    /// Open at the feet and below: a pit, hole or drop.
    Drop,
    Door,
    StairsUp,
    StairsDown,
    Floor,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MapTile {
    /// The cell this column stands for; travel targets it when it's floor.
    pub position: Position,
    pub glyph: char,
    pub kind: Kind,
    pub remembered: bool,
    pub color: u32,
    pub center: (usize, usize),
    pub step: usize,
}

/// A creature's letter: the first letter of the last word of its name, as in
/// "ruin scout" -> s. Unnamed figures are &.
pub fn creature_glyph(name: &str) -> char {
    name.split_whitespace()
        .last()
        .and_then(|word| word.chars().find(char::is_ascii_alphabetic))
        .map_or('&', |c| c.to_ascii_lowercase())
}

/// A creature's colour, the same for every creature with that name.
pub fn creature_color(name: &str) -> u32 {
    let hash = name
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    CREATURES[hash as usize % CREATURES.len()]
}

/// The pixel centre of a column relative to the player.
fn center(dx: i32, dy: i32) -> Option<(usize, usize)> {
    let col = dx + COLS / 2;
    let row = dy + ROWS / 2;
    ((0..COLS).contains(&col) && (0..ROWS).contains(&row)).then(|| {
        (
            LEFT + col as usize * STEP + STEP / 2,
            TOP + row as usize * STEP + STEP / 2,
        )
    })
}

/// The column under a pixel, relative to the player.
fn column_at(x: usize, y: usize) -> Option<(i32, i32)> {
    if x < LEFT || y < TOP {
        return None;
    }
    let col = ((x - LEFT) / STEP) as i32;
    let row = ((y - TOP) / STEP) as i32;
    ((0..COLS).contains(&col) && (0..ROWS).contains(&row))
        .then_some((col - COLS / 2, row - ROWS / 2))
}

/// Whether a cell offset from the player is inside the drawn map.
pub fn in_view(o: &Observation, position: Position) -> bool {
    center(position.x - o.position.x, position.y - o.position.y).is_some()
        && BAND.contains(&(position.z - o.position.z))
}

struct Column<'a> {
    cells: BTreeMap<i32, &'a CellView>,
}

impl Column<'_> {
    fn at(&self, dz: i32) -> Option<&CellView> {
        self.cells.get(&dz).copied()
    }
    fn open(&self, dz: i32) -> bool {
        self.at(dz).is_some_and(|c| !c.wall)
    }
    /// The column's terrain, and the cell it stands for.
    fn terrain(&self) -> Option<(Kind, &CellView)> {
        for dz in [0, 1] {
            if let Some(cell) = self.at(dz).filter(|c| c.door.is_some()) {
                return Some((Kind::Door, cell));
            }
        }
        if let Some(feet) = self.at(0) {
            return Some(if feet.wall {
                (
                    if self.open(1) {
                        Kind::LowWall
                    } else {
                        Kind::Wall
                    },
                    feet,
                )
            } else if feet.stairs_down {
                (Kind::StairsDown, feet)
            } else if feet.stairs_up {
                (Kind::StairsUp, feet)
            } else if self.open(-1) {
                (Kind::Drop, feet)
            } else {
                (Kind::Floor, feet)
            });
        }
        if let Some(head) = self.at(1) {
            return Some((if head.wall { Kind::Wall } else { Kind::Floor }, head));
        }
        self.at(-1)
            .map(|below| (if below.wall { Kind::Floor } else { Kind::Drop }, below))
    }
}

/// Every drawn column of a view, merged across heights.
pub fn tiles(o: &Observation, current: &dyn Fn(&CellView) -> bool) -> Vec<MapTile> {
    let feet = o.position;
    let mut columns: BTreeMap<(i32, i32), Column> = BTreeMap::new();
    for cell in &o.visible_cells {
        let dz = cell.position.z - feet.z;
        if !BAND.contains(&dz)
            || center(cell.position.x - feet.x, cell.position.y - feet.y).is_none()
        {
            continue;
        }
        columns
            .entry((cell.position.x, cell.position.y))
            .or_insert_with(|| Column {
                cells: BTreeMap::new(),
            })
            .cells
            .entry(dz)
            .or_insert(cell);
    }
    let mut creatures: BTreeMap<(i32, i32), &tor_protocol::ActorView> = BTreeMap::new();
    for actor in &o.visible_actors {
        if actor.id != o.self_target && BAND.contains(&(actor.position.z - feet.z)) {
            creatures
                .entry((actor.position.x, actor.position.y))
                .or_insert(actor);
        }
    }
    let mut items: BTreeMap<(i32, i32), &tor_protocol::GroundItemView> = BTreeMap::new();
    for item in &o.ground_items {
        // Items lie at the player's level or down in a drop.
        if (-2..=0).contains(&(item.position.z - feet.z)) {
            items
                .entry((item.position.x, item.position.y))
                .or_insert(item);
        }
    }
    let mut tiles = Vec::new();
    for ((x, y), column) in &columns {
        let Some((terrain, cell)) = column.terrain() else {
            continue;
        };
        let remembered = !column.cells.values().any(|c| current(c));
        let (kind, glyph, color) = if (*x, *y) == (feet.x, feet.y) {
            (Kind::Player, '@', SELF)
        } else if let Some(actor) = creatures.get(&(*x, *y)).filter(|_| !remembered) {
            (
                Kind::Creature,
                creature_glyph(&actor.name),
                creature_color(&actor.name),
            )
        } else if let Some(item) = items.get(&(*x, *y)) {
            (Kind::Item, crate::item_glyph(item.item.class), ITEM)
        } else {
            match terrain {
                Kind::Door => {
                    let open = cell.door.as_ref().is_some_and(|d| d.open);
                    (Kind::Door, if open { '/' } else { '+' }, DOOR)
                }
                Kind::Wall => (Kind::Wall, '#', TERRAIN),
                Kind::LowWall => (Kind::LowWall, '#', LOW_WALL),
                Kind::Drop => (Kind::Drop, '^', DROP),
                Kind::StairsUp => (Kind::StairsUp, '<', STAIRS),
                Kind::StairsDown => (Kind::StairsDown, '>', STAIRS),
                other => (other, '.', TERRAIN),
            }
        };
        let Some(center) = center(x - feet.x, y - feet.y) else {
            continue;
        };
        tiles.push(MapTile {
            position: cell.position,
            glyph,
            kind,
            remembered: remembered && kind != Kind::Player,
            color: if remembered && kind != Kind::Player {
                MEMORY_COLOR
            } else {
                color
            },
            center,
            step: STEP,
        });
    }
    tiles
}

/// The cell a click or cursor at this column selects: the column's cell at
/// the player's own foot level, when it's known. A column seen only at head
/// height or below isn't a place to stand.
pub fn target(o: &Observation, x: i32, y: i32) -> Option<Position> {
    let position = Position {
        x,
        y,
        z: o.position.z,
    };
    o.visible_cells
        .iter()
        .any(|cell| cell.position == position)
        .then_some(position)
}

/// The column under a pixel, as the cell it selects.
pub fn cell_at(o: &Observation, x: usize, y: usize) -> Option<Position> {
    let (dx, dy) = column_at(x, y)?;
    target(o, o.position.x + dx, o.position.y + dy)
}

/// Where a cell's column is drawn.
pub fn cell_center(o: &Observation, position: Position) -> Option<(usize, usize)> {
    if !o.visible_cells.iter().any(|c| c.position == position) {
        return None;
    }
    center(position.x - o.position.x, position.y - o.position.y)
}

/// The column offsets kept on screen; a cursor stays within them.
pub fn clamp(o: &Observation, mut position: Position) -> Position {
    position.x = position
        .x
        .clamp(o.position.x - COLS / 2, o.position.x + COLS / 2);
    position.y = position
        .y
        .clamp(o.position.y - ROWS / 2, o.position.y + ROWS / 2);
    position
}
