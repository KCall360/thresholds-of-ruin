//! The place the character is in, worked out from the disclosed cells alone:
//! its extent, its shape, and the ways out of it.
//!
//! The body stands on the open cells at its own level (z = 0) with headroom
//! above (z = 1). A column is open when its z = 0 cell is seen and open; it is
//! blocked when a wall is seen at either height. A place is the open columns
//! reachable from the character's without crossing a door or a narrow gap in
//! the walls; those are its openings. Standing in a gap, the place is the
//! passage of gaps it belongs to.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tor_protocol::*;

use super::scene::{bearing, distance};

type Column = (i32, i32);

const STEPS: [Column; 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// What the character can see of one column.
struct Columns<'a> {
    floor: BTreeMap<Column, &'a CellView>,
    head: BTreeMap<Column, &'a CellView>,
}

impl<'a> Columns<'a> {
    fn new(state: &'a StateView) -> Self {
        let mut floor = BTreeMap::new();
        let mut head = BTreeMap::new();
        for c in &state.observation.visible_cells {
            let column = (c.position.x, c.position.y);
            match c.position.z {
                0 => {
                    floor.entry(column).or_insert(c);
                }
                1 => {
                    head.entry(column).or_insert(c);
                }
                _ => {}
            }
        }
        Self { floor, head }
    }

    fn door(&self, c: Column) -> Option<&'a DoorView> {
        self.floor.get(&c).and_then(|cell| cell.door.as_ref())
    }

    /// Open to walk in, and not a doorway.
    fn open(&self, c: Column) -> bool {
        self.floor
            .get(&c)
            .is_some_and(|cell| !cell.wall && cell.door.is_none())
            && !self.head.get(&c).is_some_and(|cell| cell.wall)
    }

    fn seen(&self, c: Column) -> bool {
        self.floor.contains_key(&c) || self.head.contains_key(&c)
    }

    /// An open column in a gap at most two wide. Unseen columns close a gap
    /// too: where regions join, nothing is disclosed beside the opening.
    fn gap(&self, (x, y): Column) -> bool {
        let b = |dx: i32, dy: i32| !self.open((x + dx, y + dy));
        let o = |dx: i32, dy: i32| self.open((x + dx, y + dy));
        self.open((x, y))
            && ((b(0, -1) && b(0, 1))
                || (b(-1, 0) && b(1, 0))
                || (b(0, -1) && o(0, 1) && b(0, 2))
                || (b(0, 1) && o(0, -1) && b(0, -2))
                || (b(-1, 0) && o(1, 0) && b(2, 0))
                || (b(1, 0) && o(-1, 0) && b(-2, 0)))
    }

    fn key(&self, c: Column) -> Option<&'a str> {
        self.floor.get(&c).map(|cell| cell.key.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Opening {
    Door {
        id: u64,
        name: String,
        open: bool,
    },
    /// A narrow gap with more of a passage beyond.
    Passage,
    /// A gap opening onto somewhere wider.
    Archway,
    Stairs,
}

/// A way out of the place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Way {
    pub direction: Direction,
    pub kind: Opening,
    /// Where a journey through it ends: the first seen open cell beyond, or
    /// the opening itself.
    pub destination: Option<String>,
}

impl Way {
    /// "an archway", "a closed wooden door".
    pub fn label(&self) -> String {
        match &self.kind {
            Opening::Stairs => "stairs".into(),
            _ => super::prose::indefinite(&self.kind_name()),
        }
    }

    /// What it is, without an article: "archway", "open wooden door".
    pub fn kind_name(&self) -> String {
        match &self.kind {
            Opening::Door { name, open, .. } => {
                format!("{} {name}", if *open { "open" } else { "closed" })
            }
            Opening::Passage => "passage".into(),
            Opening::Archway => "archway".into(),
            Opening::Stairs => "stairs".into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    /// Too little wall is seen to say.
    Open,
    Passage,
    Alcove,
    Chamber,
    Hall,
}

/// The place the character is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub columns: BTreeSet<Column>,
    pub form: Form,
    pub ways: Vec<Way>,
    /// Directions in which the place goes on beyond sight.
    pub continues: Vec<Direction>,
}

impl Place {
    pub fn contains(&self, p: Position) -> bool {
        self.columns.contains(&(p.x, p.y)) && p.z == 0
    }

    pub fn ways(&self, direction: Direction) -> impl Iterator<Item = &Way> {
        self.ways.iter().filter(move |w| w.direction == direction)
    }
}

fn position((x, y): Column) -> Position {
    Position { x, y, z: 0 }
}

/// Work out the place from what the character sees.
pub fn survey(state: &StateView) -> Place {
    let cols = Columns::new(state);
    let origin = (0, 0);
    let walls = state.observation.visible_cells.iter().any(|c| c.wall);
    // With no walls in sight, gaps mean nothing: the place is the open
    // ground nearest the same authored anchor.
    let anchors: Vec<Column> = state
        .observation
        .visible_cells
        .iter()
        .filter(|c| c.place_hint && !c.wall && c.position.z == 0)
        .map(|c| (c.position.x, c.position.y))
        .collect();
    let nearest = |(x, y): Column| {
        anchors
            .iter()
            .min_by_key(|(ax, ay)| ((ax - x).abs() + (ay - y).abs(), *ax, *ay))
            .copied()
    };
    let home = nearest(origin);
    let in_gap = walls && cols.gap(origin);
    // Gaps belong to a passage only when standing in one.
    let member = |c: Column| {
        cols.open(c)
            && if walls {
                cols.gap(c) == in_gap
            } else {
                nearest(c) == home
            }
    };
    let mut columns = BTreeSet::from([origin]);
    let mut queue = VecDeque::from([origin]);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in STEPS {
            let next = (x + dx, y + dy);
            if !columns.contains(&next) && member(next) {
                columns.insert(next);
                queue.push_back(next);
            }
        }
    }
    // Openings: doors and open columns beside the place that aren't part of
    // it, grouped when they touch.
    let mut edge: BTreeSet<Column> = BTreeSet::new();
    let mut continues = BTreeSet::new();
    for &(x, y) in &columns {
        for (dx, dy) in STEPS {
            let next = (x + dx, y + dy);
            if columns.contains(&next) {
                continue;
            }
            if cols.door(next).is_some() || (walls && cols.open(next)) {
                edge.insert(next);
            } else if !cols.seen(next) {
                // Only where some of the place is seen that way, so there
                // is somewhere to walk toward the dark.
                if let Some(d) = bearing(position(next))
                    .filter(|d| columns.iter().any(|c| bearing(position(*c)) == Some(*d)))
                {
                    continues.insert(direction_order(d));
                }
            }
        }
    }
    let mut ways = Vec::new();
    let mut left = edge.clone();
    while let Some(&start) = left.iter().next() {
        let mut group = vec![start];
        left.remove(&start);
        let mut i = 0;
        while i < group.len() {
            let (x, y) = group[i];
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let next = (x + dx, y + dy);
                    if left.remove(&next) {
                        group.push(next);
                    }
                }
            }
            i += 1;
        }
        ways.push(way(&cols, &columns, &group, in_gap));
    }
    if let Some(here) = cols.floor.get(&origin) {
        for (up, there) in [(true, here.stairs_up), (false, here.stairs_down)] {
            if there {
                let z = if up { 1 } else { -1 };
                let landing = state
                    .observation
                    .visible_cells
                    .iter()
                    .filter(|c| !c.wall && c.position.x == 0 && c.position.y == 0)
                    .filter(|c| c.position.z.signum() == z)
                    .min_by_key(|c| c.position.z.abs());
                ways.push(Way {
                    direction: if up { Direction::Up } else { Direction::Down },
                    kind: Opening::Stairs,
                    destination: landing.map(|c| c.key.clone()),
                });
            }
        }
    }
    ways.sort_by_key(|w| direction_order(w.direction));
    let form = if !walls {
        Form::Open
    } else if in_gap {
        Form::Passage
    } else {
        let (min_x, max_x) = bounds(columns.iter().map(|c| c.0));
        let (min_y, max_y) = bounds(columns.iter().map(|c| c.1));
        let (w, h) = (max_x - min_x + 1, max_y - min_y + 1);
        let n = columns.len();
        if (w.min(h) <= 2 && w.max(h) >= 4) || (w.min(h) <= 1) {
            Form::Passage
        } else if n <= 6 {
            Form::Alcove
        } else if n <= 40 {
            Form::Chamber
        } else {
            Form::Hall
        }
    };
    Place {
        columns,
        form,
        ways,
        continues: continues.into_iter().map(direction_from).collect(),
    }
}

fn bounds(values: impl Iterator<Item = i32>) -> (i32, i32) {
    values.fold((i32::MAX, i32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
}

const ORDER: [Direction; 10] = [
    Direction::North,
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::South,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
    Direction::Up,
    Direction::Down,
];

fn direction_order(d: Direction) -> usize {
    ORDER.iter().position(|o| *o == d).unwrap_or(ORDER.len())
}

fn direction_from(i: usize) -> Direction {
    ORDER[i]
}

/// One opening: its kind, where it leads, and its bearing.
fn way(cols: &Columns, place: &BTreeSet<Column>, group: &[Column], in_gap: bool) -> Way {
    let door = group.iter().find_map(|c| cols.door(*c));
    // Beyond: open columns reached from the opening, away from the place,
    // within a few steps.
    let mut seen: BTreeSet<Column> = group.iter().copied().collect();
    let mut frontier: Vec<Column> = group.to_vec();
    let mut beyond = Vec::new();
    for _ in 0..3 {
        let mut next = Vec::new();
        for &(x, y) in &frontier {
            for (dx, dy) in STEPS {
                let c = (x + dx, y + dy);
                if !place.contains(&c) && seen.insert(c) && cols.open(c) {
                    next.push(c);
                    beyond.push(c);
                }
            }
        }
        frontier = next;
    }
    let far = |c: &&Column| distance(position(**c));
    let destination = beyond
        .iter()
        .max_by_key(|c| (far(c), **c))
        .or_else(|| group.iter().filter(|c| cols.open(**c)).max_by_key(far))
        .and_then(|c| cols.key(*c))
        .map(str::to_owned);
    let nearest = group
        .iter()
        .min_by_key(|c| (distance(position(**c)), **c))
        .copied()
        .unwrap_or((0, 0));
    let direction = bearing(position(nearest))
        .or_else(|| beyond.first().and_then(|c| bearing(position(*c))))
        .unwrap_or(Direction::North);
    let kind = match door {
        Some(d) => Opening::Door {
            id: d.id,
            name: if d.name.trim().is_empty() {
                "door".into()
            } else {
                crate::safe(&d.name).to_lowercase()
            },
            open: d.open,
        },
        None if !in_gap && beyond.iter().any(|c| cols.gap(*c)) => Opening::Passage,
        None if !in_gap && group.iter().all(|c| cols.gap(*c)) && beyond.is_empty() => {
            Opening::Passage
        }
        None => Opening::Archway,
    };
    Way {
        direction,
        kind,
        destination,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view from a map: `#` wall (both heights), `.` open, `+` closed door,
    /// `'` open door, `@` the character on open floor, space unseen.
    fn view(map: &[&str]) -> StateView {
        let mut cells = Vec::new();
        let at = map
            .iter()
            .enumerate()
            .find_map(|(y, row)| row.find('@').map(|x| (x as i32, y as i32)))
            .expect("an @");
        for (y, row) in map.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                let (x, y) = (x as i32 - at.0, y as i32 - at.1);
                if ch == ' ' {
                    continue;
                }
                for z in [0, 1] {
                    let door = matches!(ch, '+' | '\'') && z == 0;
                    cells.push(serde_json::json!({
                        "key": format!("{x},{y},{z}"),
                        "position": {"x": x, "y": y, "z": z},
                        "wall": ch == '#',
                        "material": "stone",
                        "place_hint": false,
                        "stairs_up": false,
                        "stairs_down": false,
                        "door": door.then(|| serde_json::json!({
                            "id": 9, "name": "oak door", "description": "",
                            "open": ch == '\'', "reachable": false, "approaches": []
                        })),
                    }));
                }
            }
        }
        serde_json::from_value(serde_json::json!({
            "wizard_game": false, "revision": 0, "observation": {
                "actor": 1, "tick": 0, "position": {"x": 0, "y": 0, "z": 0},
                "ready": true, "places": [], "visible_cells": cells,
                "ground_items": [], "inventory": [], "visible_actors": []
            }
        }))
        .unwrap()
    }

    #[test]
    fn a_chamber_ends_at_a_gap_and_the_way_leads_beyond_it() {
        let place = survey(&view(&[
            "#######  ",
            "#.....#  ",
            "#..@.....",
            "#.....#  ",
            "#######  ",
        ]));
        assert_eq!(place.form, Form::Chamber);
        assert_eq!(place.columns.len(), 15);
        assert_eq!(place.ways.len(), 1);
        let way = &place.ways[0];
        assert_eq!(way.direction, Direction::East);
        assert_eq!(way.kind, Opening::Passage);
        // The farthest seen cell beyond the gap.
        assert_eq!(way.destination.as_deref(), Some("5,0,0"));
        assert_eq!(place.continues, []);
    }

    #[test]
    fn standing_in_a_passage_its_ends_are_the_ways() {
        let place = survey(&view(&[
            "#####   #####",
            "#...#####...#",
            "#.....@.....#",
            "#...#####...#",
            "#####   #####",
        ]));
        assert_eq!(place.form, Form::Passage);
        let directions: Vec<_> = place.ways.iter().map(|w| w.direction).collect();
        assert_eq!(directions, [Direction::East, Direction::West]);
        assert!(place.ways.iter().all(|w| w.kind == Opening::Archway));
    }

    #[test]
    fn doors_are_ways_and_a_room_seen_in_part_goes_on() {
        let place = survey(&view(&["#####", "#...'....", "#.@.#", "#...+", "#..."]));
        let doors: Vec<_> = place
            .ways
            .iter()
            .map(|w| (w.direction, w.label()))
            .collect();
        assert_eq!(
            doors,
            [
                (Direction::NorthEast, "an open oak door".to_owned()),
                (Direction::SouthEast, "a closed oak door".to_owned())
            ]
        );
        assert_eq!(place.continues, [Direction::SouthEast, Direction::South]);
    }
}
