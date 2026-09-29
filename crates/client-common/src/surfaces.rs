//! Floors, ceilings and walls are ordinary seen solid cells; the server sends no
//! separate surface facts. A solid cell's role follows from the open cells seen
//! next to it: an open cell above makes it a floor, one below a ceiling, and
//! one beside it a wall. A cell can have several roles.
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::{CellView, Position};

/// How far straight up [`ceiling_above`] looks, in cells.
const CEILING_SEARCH: i32 = 16;

/// Materials of the seen solid cells, by role.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Roles<'a> {
    pub floors: BTreeSet<&'a str>,
    pub ceilings: BTreeSet<&'a str>,
    pub walls: BTreeSet<&'a str>,
}

/// A solid cell's material, or a neutral phrase when the server gives none.
pub fn material(cell: &CellView) -> &str {
    if cell.material.is_empty() {
        "unremarkable material"
    } else {
        &cell.material
    }
}

/// Cells by relative position; the first occurrence wins, as in rendering.
fn index(cells: &[CellView]) -> BTreeMap<(i32, i32, i32), &CellView> {
    let mut map = BTreeMap::new();
    for cell in cells {
        let p = cell.position;
        map.entry((p.x, p.y, p.z)).or_insert(cell);
    }
    map
}

fn open(map: &BTreeMap<(i32, i32, i32), &CellView>, key: (i32, i32, i32)) -> bool {
    map.get(&key).is_some_and(|cell| !cell.wall)
}

/// Classify every seen solid cell by the open cells seen next to it.
pub fn roles(cells: &[CellView]) -> Roles<'_> {
    let map = index(cells);
    let mut roles = Roles::default();
    for (&(x, y, z), cell) in &map {
        if !cell.wall {
            continue;
        }
        if open(&map, (x, y, z + 1)) {
            roles.floors.insert(material(cell));
        }
        if open(&map, (x, y, z - 1)) {
            roles.ceilings.insert(material(cell));
        }
        if [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .into_iter()
            .any(|(dx, dy)| open(&map, (x + dx, y + dy, z)))
        {
            roles.walls.insert(material(cell));
        }
    }
    roles
}

/// The seen solid cell directly below `at`, if any.
pub fn floor_below(cells: &[CellView], at: Position) -> Option<&CellView> {
    cells
        .iter()
        .find(|c| c.wall && c.position == Position { z: at.z - 1, ..at })
}

/// The nearest seen solid cell straight above `at`, with its distance in cells,
/// if every cell between them was seen open. An unseen gap means the ceiling
/// isn't known.
pub fn ceiling_above(cells: &[CellView], at: Position) -> Option<(&CellView, u32)> {
    let map = index(cells);
    for distance in 1..=CEILING_SEARCH {
        let cell = map.get(&(at.x, at.y, at.z + distance))?;
        if cell.wall {
            return Some((cell, distance as u32));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(x: i32, y: i32, z: i32, wall: bool, material: &str) -> CellView {
        CellView {
            door: None,
            material: material.into(),
            key: format!("{x},{y},{z}"),
            stairs_up: false,
            stairs_down: false,
            position: Position { x, y, z },
            wall,
            place_hint: false,
        }
    }

    /// A column: stone floor, two open cells, a wooden ceiling; and a marble
    /// wall beside the lower open cell.
    fn room() -> Vec<CellView> {
        vec![
            cell(0, 0, -1, true, "stone"),
            cell(0, 0, 0, false, ""),
            cell(0, 0, 1, false, ""),
            cell(0, 0, 2, true, "wood"),
            cell(1, 0, 0, true, "marble"),
        ]
    }

    #[test]
    fn solid_cells_take_roles_from_their_open_neighbours() {
        let cells = room();
        let roles = roles(&cells);
        assert_eq!(roles.floors, BTreeSet::from(["stone"]));
        assert_eq!(roles.ceilings, BTreeSet::from(["wood"]));
        assert_eq!(roles.walls, BTreeSet::from(["marble"]));
        let origin = Position { x: 0, y: 0, z: 0 };
        assert_eq!(
            floor_below(&cells, origin).map(|c| c.material.as_str()),
            Some("stone")
        );
        let (ceiling, distance) = ceiling_above(&cells, origin).unwrap();
        assert_eq!((ceiling.material.as_str(), distance), ("wood", 2));
    }

    #[test]
    fn an_unseen_gap_hides_the_ceiling_and_a_missing_floor_is_none() {
        let mut cells = room();
        cells.retain(|c| c.position.z != 1);
        let origin = Position { x: 0, y: 0, z: 0 };
        assert!(ceiling_above(&cells, origin).is_none());
        cells.retain(|c| c.position.z != -1);
        assert!(floor_below(&cells, origin).is_none());
        assert!(roles(&cells).floors.is_empty());
    }
}
