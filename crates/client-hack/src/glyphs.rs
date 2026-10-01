//! Static glyph and word table. Lookup is a binary search of the full id, then
//! each shorter dotted prefix, then the structural default. No palette.

/// Grey for a fact that is only in the remembered chart.
pub const REMEMBERED_COLOR: u32 = 0x626262;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub color: u32,
    pub word: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Structure {
    Wall,
    Floor,
    DoorClosed,
    DoorOpen,
    StairsUp,
    StairsDown,
    Item,
    Actor,
    Pit,
}

struct Row {
    id: &'static str,
    glyph: Glyph,
}

/// Sorted by id. These are the ids `scenarios/tests/generated-filler` sends.
static ROWS: &[Row] = &[
    row("creature.rat", 'r', 0xC4A574, "rat"),
    row("item.coin", '$', 0xE6C34A, "coin"),
    row("terrain.floor.cave", '.', 0x6E7A55, "cave floor"),
    row("terrain.floor.marble", '.', 0xD9E2EA, "marble floor"),
    row("terrain.floor.stone", '.', 0x9AA7A0, "stone floor"),
    row("terrain.wall.cave", '#', 0x7D6B52, "cave wall"),
    row("terrain.wall.marble", '#', 0xE6E6E6, "marble wall"),
    row("terrain.wall.stone", '#', 0xB7B7B7, "stone wall"),
];

const fn row(id: &'static str, ch: char, color: u32, word: &'static str) -> Row {
    Row {
        id,
        glyph: Glyph { ch, color, word },
    }
}

const fn rows_are_sorted() -> bool {
    let mut index = 1;
    while index < ROWS.len() {
        let previous = ROWS[index - 1].id.as_bytes();
        let current = ROWS[index].id.as_bytes();
        let mut byte = 0;
        let mut ordered = false;
        while byte < previous.len() && byte < current.len() {
            if previous[byte] < current[byte] {
                ordered = true;
                break;
            }
            if previous[byte] > current[byte] {
                return false;
            }
            byte += 1;
        }
        if !ordered && previous.len() >= current.len() {
            return false;
        }
        index += 1;
    }
    true
}

const _: () = assert!(rows_are_sorted());

/// Same predicate as the private `scenario_package::asset_id` in `tor-server`.
/// Clients do not depend on that crate. A rejected string is not drawn as text.
fn asset_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

fn structural(structure: Structure) -> Glyph {
    match structure {
        Structure::Wall => Glyph {
            ch: '#',
            color: 0xC8C8C8,
            word: "wall",
        },
        Structure::Floor => Glyph {
            ch: '.',
            color: 0x6A7A72,
            word: "floor",
        },
        Structure::DoorClosed => Glyph {
            ch: '+',
            color: 0xC4A35A,
            word: "closed door",
        },
        Structure::DoorOpen => Glyph {
            ch: '/',
            color: 0x8D6E3A,
            word: "open door",
        },
        Structure::StairsUp => Glyph {
            ch: '<',
            color: 0x8CBAFA,
            word: "stairs up",
        },
        Structure::StairsDown => Glyph {
            ch: '>',
            color: 0x8CBAFA,
            word: "stairs down",
        },
        Structure::Item => Glyph {
            ch: '!',
            color: 0xEDC579,
            word: "object",
        },
        Structure::Actor => Glyph {
            ch: '&',
            color: 0xEF958C,
            word: "creature",
        },
        Structure::Pit => Glyph {
            ch: '^',
            color: 0xD07A4A,
            word: "pit",
        },
    }
}

fn lookup(id: &str) -> Option<&'static Row> {
    ROWS.binary_search_by_key(&id, |row| row.id)
        .ok()
        .map(|index| &ROWS[index])
}

/// Full id, then each shorter dotted prefix, then the structural default.
pub fn resolve(asset: Option<&str>, structure: Structure) -> Glyph {
    if let Some(id) = asset.filter(|id| asset_id(id)) {
        let mut current = Some(id);
        while let Some(name) = current {
            if let Some(row) = lookup(name) {
                return row.glyph;
            }
            current = name.rsplit_once('.').map(|(prefix, _)| prefix);
        }
    }
    structural(structure)
}
