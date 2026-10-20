//! One map for every height. Each column of cells around the player is drawn
//! as what stands at the player's own level there: a wall, a low wall, a
//! floor, a drop, a door, stairs, an item or a creature. Heights well above
//! or below the player (a stair's far landing, a distant ceiling) are left out.
use std::collections::{BTreeMap, BTreeSet};
use tor_client_common::{presentation, presentation::AsciiAppearance, Palette};
use tor_protocol::{
    ActorView, CellView, ItemClass, ItemView, MapDoorAppearance, MapItemAppearance, Observation,
    Position, RememberedMapCell,
};

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
const CREATURE: u32 = 0xef958c;

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

/// A disclosed creature's visual, independent of its authored name.
pub fn creature(palette: &Palette, asset: Option<&str>) -> AsciiAppearance {
    presentation::resolve(palette, asset).map_or(
        AsciiAppearance {
            glyph: '&',
            color: CREATURE,
        },
        |entry| entry.ascii,
    )
}

/// One disclosed occupant per drawn column, shared by map, look and bump input.
/// Body adjacency is only an input-selection hint: terrain, portal contact and
/// actual melee reach remain server authority.
pub(crate) struct CreatureColumns<'a> {
    actors: BTreeMap<(i32, i32), &'a ActorView>,
    body: BTreeSet<Position>,
}

impl<'a> CreatureColumns<'a> {
    pub(crate) fn new(view: &'a Observation) -> Self {
        // The origin is omitted from visible_actors. Other own occurrences can
        // be distant portal images, so only the origin-connected disclosed
        // component can extend the local input envelope.
        let mut unseen: BTreeSet<_> = view
            .visible_actors
            .iter()
            .filter(|actor| actor.id == view.self_target)
            .map(|actor| actor.position)
            .collect();
        unseen.remove(&view.position);
        let mut body = BTreeSet::from([view.position]);
        let mut pending = vec![view.position];
        while let Some(from) = pending.pop() {
            for (dx, dy, dz) in [
                (1, 0, 0),
                (-1, 0, 0),
                (0, 1, 0),
                (0, -1, 0),
                (0, 0, 1),
                (0, 0, -1),
            ] {
                let (Some(x), Some(y), Some(z)) = (
                    from.x.checked_add(dx),
                    from.y.checked_add(dy),
                    from.z.checked_add(dz),
                ) else {
                    continue;
                };
                let next = Position { x, y, z };
                if unseen.remove(&next) {
                    body.insert(next);
                    pending.push(next);
                }
            }
        }
        let mut result = Self {
            actors: BTreeMap::new(),
            body,
        };
        for actor in &view.visible_actors {
            if actor.id == view.self_target
                || !actor
                    .position
                    .z
                    .checked_sub(view.position.z)
                    .is_some_and(|height| BAND.contains(&height))
            {
                continue;
            }
            // Prefer the actual step-height occupant, then a nearby body
            // occurrence over a high flyer. Equal priorities keep disclosed
            // source order; opaque actor tokens carry no selection meaning.
            let priority = |candidate: &ActorView| {
                (
                    candidate.position.z != view.position.z,
                    !result.near_body(candidate.position),
                    candidate.position.z.abs_diff(view.position.z),
                )
            };
            let column = (actor.position.x, actor.position.y);
            if result
                .actors
                .get(&column)
                .is_none_or(|current| priority(actor) < priority(current))
            {
                result.actors.insert(column, actor);
            }
        }
        result
    }

    pub(crate) fn actor(&self, x: i32, y: i32) -> Option<&'a ActorView> {
        self.actors.get(&(x, y)).copied()
    }

    pub(crate) fn near_body(&self, position: Position) -> bool {
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    if (dx, dy, dz) == (0, 0, 0) {
                        continue;
                    }
                    let (Some(x), Some(y), Some(z)) = (
                        position.x.checked_add(dx),
                        position.y.checked_add(dy),
                        position.z.checked_add(dz),
                    ) else {
                        continue;
                    };
                    if self.body.contains(&Position { x, y, z }) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// Map, look and inventory use the same disclosed item asset. Its disclosed
/// class is the safe fallback, including for a concealed appearance pool.
pub fn item(palette: &Palette, item: &ItemView) -> AsciiAppearance {
    item_appearance(palette, item.class, item.asset.as_deref())
}

fn item_appearance(palette: &Palette, class: ItemClass, asset: Option<&str>) -> AsciiAppearance {
    presentation::resolve(palette, asset).map_or(
        AsciiAppearance {
            glyph: crate::item_glyph(class),
            color: ITEM,
        },
        |entry| entry.ascii,
    )
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
pub(crate) fn column_at(x: usize, y: usize) -> Option<(i32, i32)> {
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
    cells: BTreeMap<i32, &'a RememberedMapCell>,
}

impl Column<'_> {
    fn at(&self, dz: i32) -> Option<&RememberedMapCell> {
        self.cells.get(&dz).copied()
    }
    fn open(&self, dz: i32) -> bool {
        self.at(dz).is_some_and(|c| !c.wall)
    }
    /// The column's terrain, and the cell it stands for.
    fn terrain(&self) -> Option<(Kind, &RememberedMapCell)> {
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
pub fn tiles(
    o: &Observation,
    palette: &Palette,
    current: &dyn Fn(&CellView) -> bool,
) -> Vec<MapTile> {
    let cells = appearance_cells(o);
    let current_positions: std::collections::BTreeSet<_> = o
        .visible_cells
        .iter()
        .filter(|c| current(c))
        .map(|c| c.position)
        .collect();
    appearance_tiles(o, &cells, palette, &|p| current_positions.contains(&p))
}

pub(crate) fn appearance_cells(o: &Observation) -> Vec<RememberedMapCell> {
    let mut items: BTreeMap<Position, Vec<&tor_protocol::GroundItemView>> = BTreeMap::new();
    for item in &o.ground_items {
        items.entry(item.position).or_default().push(item);
    }
    o.visible_cells
        .iter()
        .map(|cell| RememberedMapCell {
            key: cell.key.clone(),
            position: cell.position,
            wall: cell.wall,
            material: cell.material.clone(),
            asset: cell.asset.clone(),
            place_hint: cell.place_hint,
            stairs_up: cell.stairs_up,
            stairs_down: cell.stairs_down,
            door: cell.door.as_ref().map(|d| MapDoorAppearance {
                name: d.name.clone(),
                description: d.description.clone(),
                open: d.open,
                asset: d.asset.clone(),
            }),
            ground_items: items
                .get(&cell.position)
                .into_iter()
                .flatten()
                .map(|g| MapItemAppearance {
                    class: g.item.class,
                    name: g.item.name.clone(),
                    description: g.item.description.clone(),
                    appearance: g.item.appearance.clone(),
                    identified: g.item.identified,
                    quantity: g.item.quantity,
                    asset: g.item.asset.clone(),
                })
                .collect(),
        })
        .collect()
}

/// Appearance-only map input keeps remembered items and doors incapable of actions.
pub(crate) fn appearance_tiles(
    o: &Observation,
    cells: &[RememberedMapCell],
    palette: &Palette,
    current: &dyn Fn(Position) -> bool,
) -> Vec<MapTile> {
    let feet = o.position;
    let mut columns: BTreeMap<(i32, i32), Column> = BTreeMap::new();
    for cell in cells {
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
    let creatures = CreatureColumns::new(o);
    let mut items: BTreeMap<(i32, i32), (ItemClass, Option<&str>)> = BTreeMap::new();
    // Current piles retain their disclosed order and win over every stale
    // occurrence in this column, even a lower remembered height plane.
    for item in &o.ground_items {
        if (-2..=0).contains(&(item.position.z - feet.z)) {
            items
                .entry((item.position.x, item.position.y))
                .or_insert((item.item.class, item.item.asset.as_deref()));
        }
    }
    for cell in cells {
        if (-2..=0).contains(&(cell.position.z - feet.z)) {
            if let Some(item) = cell.ground_items.first() {
                items
                    .entry((cell.position.x, cell.position.y))
                    .or_insert((item.class, item.asset.as_deref()));
            }
        }
    }
    let mut tiles = Vec::new();
    for ((x, y), column) in &columns {
        let Some((terrain, cell)) = column.terrain() else {
            continue;
        };
        let remembered = !column.cells.values().any(|c| current(c.position));
        let (kind, glyph, color) = if (*x, *y) == (feet.x, feet.y) {
            let color = o
                .visible_actors
                .iter()
                .find(|actor| actor.id == o.self_target)
                .and_then(|actor| presentation::resolve(palette, actor.asset.as_deref()))
                .map_or(SELF, |entry| entry.ascii.color);
            (Kind::Player, '@', color)
        } else if let Some(actor) = creatures.actor(*x, *y).filter(|_| !remembered) {
            let look = creature(palette, actor.asset.as_deref());
            (Kind::Creature, look.glyph, look.color)
        } else if let Some(item) = items.get(&(*x, *y)) {
            let look = item_appearance(palette, item.0, item.1);
            (Kind::Item, look.glyph, look.color)
        } else {
            // Geometry and open/closed state remain authoritative. Cosmetic
            // assets may tint terrain but cannot turn a drop into floor or hide
            // stairs. Low walls and drops keep their existing state cues.
            let tint = |asset: Option<&str>, fallback| {
                presentation::resolve(palette, asset).map_or(fallback, |entry| entry.ascii.color)
            };
            match terrain {
                Kind::Door => {
                    let open = cell.door.as_ref().is_some_and(|d| d.open);
                    (
                        Kind::Door,
                        if open { '/' } else { '+' },
                        tint(
                            cell.door.as_ref().and_then(|door| door.asset.as_deref()),
                            DOOR,
                        ),
                    )
                }
                Kind::Wall => (Kind::Wall, '#', tint(cell.asset.as_deref(), TERRAIN)),
                Kind::LowWall => (Kind::LowWall, '#', LOW_WALL),
                Kind::Drop => (Kind::Drop, '^', DROP),
                Kind::StairsUp => (Kind::StairsUp, '<', STAIRS),
                Kind::StairsDown => (Kind::StairsDown, '>', STAIRS),
                other => (
                    other,
                    '.',
                    tint(
                        column
                            .at(-1)
                            .filter(|floor| floor.wall)
                            .and_then(|floor| floor.asset.as_deref())
                            .or(cell.asset.as_deref()),
                        TERRAIN,
                    ),
                ),
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
