//! Structural glyph defaults. Asset rows are added later; stairs, doors, and pits
//! already force their characters so a floor-asset row cannot replace them.

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

/// Sorted by id. Empty until asset rows are added; lookup still walks prefixes.
static ROWS: &[Row] = &[];

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
