//! One glyph per observer-frame column. Standing plane is `observation.position.z`.
use crate::glyphs::{resolve, Structure, REMEMBERED_COLOR};
use std::collections::{BTreeMap, BTreeSet};
use tor_client_common::RememberedCell;
use tor_protocol::{ActorId, CellView, Observation, Position};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColumnGlyph {
    pub ch: char,
    pub color: u32,
    pub remembered: bool,
    pub word: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DrawnColumn {
    pub x: i32,
    pub y: i32,
    pub glyph: ColumnGlyph,
}

struct Fact {
    wall: bool,
    door_open: Option<bool>,
    door_asset: Option<String>,
    stairs_up: bool,
    stairs_down: bool,
    asset: Option<String>,
    key: String,
    current: bool,
}

struct ActorMark {
    id: ActorId,
    asset: Option<String>,
}

struct ItemMark {
    id: u64,
    asset: Option<String>,
}

struct Index {
    cells: BTreeMap<(i32, i32, i32), Fact>,
    actors: BTreeMap<(i32, i32), ActorMark>,
    items: BTreeMap<(i32, i32), ItemMark>,
}

fn rank(fact: &Fact) -> u8 {
    if fact.wall {
        0
    } else if fact.door_open == Some(false) {
        1
    } else if fact.door_open == Some(true) {
        2
    } else if fact.stairs_up {
        3
    } else if fact.stairs_down {
        4
    } else {
        5
    }
}

fn better(candidate: &Fact, old: &Fact) -> bool {
    (rank(candidate), candidate.key.as_str()) < (rank(old), old.key.as_str())
}

fn insert_cell(map: &mut BTreeMap<(i32, i32, i32), Fact>, fact: Fact, position: Position) {
    let coord = (position.x, position.y, position.z);
    match map.get(&coord) {
        Some(old) if !better(&fact, old) => {}
        _ => {
            map.insert(coord, fact);
        }
    }
}

fn from_cell(cell: &CellView, current: bool) -> Fact {
    let remembered = !current;
    Fact {
        wall: cell.wall,
        door_open: cell.door.as_ref().map(|door| door.open),
        door_asset: if remembered {
            None
        } else {
            cell.door.as_ref().and_then(|door| door.asset.clone())
        },
        stairs_up: cell.stairs_up,
        stairs_down: cell.stairs_down,
        asset: if remembered { None } else { cell.asset.clone() },
        key: cell.key.clone(),
        current,
    }
}

fn from_remembered(cell: &RememberedCell) -> Fact {
    Fact {
        wall: cell.wall,
        door_open: cell.door.as_ref().map(|door| door.open),
        door_asset: None,
        stairs_up: cell.stairs_up,
        stairs_down: cell.stairs_down,
        asset: None,
        key: cell.key.clone(),
        current: false,
    }
}

impl Index {
    fn build(observation: &Observation, chart: &[&RememberedCell]) -> Self {
        let mut cells = BTreeMap::new();
        for cell in &observation.visible_cells {
            insert_cell(&mut cells, from_cell(cell, true), cell.position);
        }
        for cell in chart {
            let position = cell.position;
            if cells.contains_key(&(position.x, position.y, position.z)) {
                continue;
            }
            insert_cell(&mut cells, from_remembered(cell), position);
        }
        let mut actors = BTreeMap::new();
        for actor in &observation.visible_actors {
            if actor.id == observation.actor {
                continue;
            }
            let coord = (actor.position.x, actor.position.y);
            let replace = actors
                .get(&coord)
                .is_none_or(|known: &ActorMark| actor.id < known.id);
            if replace {
                actors.insert(
                    coord,
                    ActorMark {
                        id: actor.id,
                        asset: actor.asset.clone(),
                    },
                );
            }
        }
        let mut items = BTreeMap::new();
        for item in &observation.ground_items {
            let coord = (item.position.x, item.position.y);
            let replace = items
                .get(&coord)
                .is_none_or(|known: &ItemMark| item.item.id < known.id);
            if replace {
                items.insert(
                    coord,
                    ItemMark {
                        id: item.item.id,
                        asset: item.item.asset.clone(),
                    },
                );
            }
        }
        Self {
            cells,
            actors,
            items,
        }
    }

    fn cell(&self, x: i32, y: i32, z: i32) -> Option<&Fact> {
        self.cells.get(&(x, y, z))
    }

    fn glyph(&self, observation: &Observation, x: i32, y: i32) -> Option<ColumnGlyph> {
        let z0 = observation.position.z;
        if (x, y) == (observation.position.x, observation.position.y) {
            return Some(ColumnGlyph {
                ch: '@',
                color: 0xFFFFFF,
                remembered: false,
                word: "you",
            });
        }
        if let Some(standing) = self.cell(x, y, z0) {
            if standing.wall || standing.door_open == Some(false) {
                return Some(paint_obstacle(standing));
            }
        }
        if let Some(actor) = self.actors.get(&(x, y)) {
            return Some(paint_resolved(
                actor.asset.as_deref(),
                Structure::Actor,
                false,
            ));
        }
        if let Some(item) = self.items.get(&(x, y)) {
            return Some(paint_resolved(
                item.asset.as_deref(),
                Structure::Item,
                false,
            ));
        }
        if seen_missing_floor(self, x, y, z0) {
            let standing = self.cell(x, y, z0).expect("standing cell");
            let glyph = resolve(None, Structure::Pit);
            return Some(finish(glyph, '^', !standing.current));
        }
        self.cell(x, y, z0).map(paint_floor)
    }
}

fn seen_open(fact: &Fact) -> bool {
    !fact.wall && fact.door_open != Some(false)
}

fn seen_missing_floor(index: &Index, x: i32, y: i32, z0: i32) -> bool {
    let Some(lower_z) = z0.checked_sub(1) else {
        return false;
    };
    match (index.cell(x, y, z0), index.cell(x, y, lower_z)) {
        (Some(standing), Some(lower)) => seen_open(standing) && seen_open(lower),
        _ => false,
    }
}

fn finish(glyph: crate::Glyph, ch: char, remembered: bool) -> ColumnGlyph {
    ColumnGlyph {
        ch,
        color: if remembered {
            REMEMBERED_COLOR
        } else {
            glyph.color
        },
        remembered,
        word: glyph.word,
    }
}

fn paint_resolved(asset: Option<&str>, structure: Structure, remembered: bool) -> ColumnGlyph {
    let glyph = resolve(if remembered { None } else { asset }, structure);
    let ch = glyph.ch;
    finish(glyph, ch, remembered)
}

fn paint_door(fact: &Fact, closed: bool) -> ColumnGlyph {
    let structure = if closed {
        Structure::DoorClosed
    } else {
        Structure::DoorOpen
    };
    let forced = if closed { '+' } else { '/' };
    if !fact.current {
        let glyph = resolve(None, structure);
        return finish(glyph, forced, true);
    }
    let row = resolve(fact.door_asset.as_deref(), structure);
    if row.ch == forced {
        finish(row, forced, false)
    } else {
        let fallback = resolve(None, structure);
        finish(fallback, forced, false)
    }
}

fn paint_obstacle(fact: &Fact) -> ColumnGlyph {
    if fact.wall {
        paint_resolved(fact.asset.as_deref(), Structure::Wall, !fact.current)
    } else {
        paint_door(fact, true)
    }
}

fn paint_floor(fact: &Fact) -> ColumnGlyph {
    if fact.door_open == Some(true) {
        return paint_door(fact, false);
    }
    if fact.stairs_up {
        let glyph = resolve(None, Structure::StairsUp);
        return finish(glyph, '<', !fact.current);
    }
    if fact.stairs_down {
        let glyph = resolve(None, Structure::StairsDown);
        return finish(glyph, '>', !fact.current);
    }
    paint_resolved(fact.asset.as_deref(), Structure::Floor, !fact.current)
}

pub fn column_glyph(
    observation: &Observation,
    chart: &[&RememberedCell],
    x: i32,
    y: i32,
) -> Option<ColumnGlyph> {
    Index::build(observation, chart).glyph(observation, x, y)
}

pub fn map_columns(observation: &Observation, chart: &[&RememberedCell]) -> Vec<DrawnColumn> {
    let index = Index::build(observation, chart);
    let mut columns = BTreeSet::new();
    for cell in &observation.visible_cells {
        columns.insert((cell.position.x, cell.position.y));
    }
    for actor in &observation.visible_actors {
        columns.insert((actor.position.x, actor.position.y));
    }
    for item in &observation.ground_items {
        columns.insert((item.position.x, item.position.y));
    }
    for cell in chart {
        columns.insert((cell.position.x, cell.position.y));
    }
    columns
        .into_iter()
        .filter_map(|(x, y)| {
            index
                .glyph(observation, x, y)
                .map(|glyph| DrawnColumn { x, y, glyph })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::*;

    fn view_at(z: i32) -> Observation {
        Observation {
            combat: None,
            motion: None,
            places: Vec::new(),
            actor: ActorId(1),
            tick: 0,
            position: Position { x: 0, y: 0, z },
            visible_cells: Vec::new(),
            ground_items: Vec::new(),
            inventory: Vec::new(),
            visible_actors: Vec::new(),
            ready: true,
        }
    }

    fn cell(key: &str, x: i32, y: i32, z: i32, wall: bool) -> CellView {
        CellView {
            asset: None,
            door: None,
            material: String::new(),
            key: key.into(),
            stairs_up: false,
            stairs_down: false,
            position: Position { x, y, z },
            wall,
            place_hint: false,
        }
    }

    fn actor(id: u64, x: i32, y: i32, z: i32) -> ActorView {
        ActorView {
            asset: None,
            name: String::new(),
            description: String::new(),
            id: ActorId(id),
            position: Position { x, y, z },
        }
    }

    fn item(id: u64, x: i32, y: i32, z: i32) -> GroundItemView {
        GroundItemView {
            reachable: true,
            item: ItemView {
                quantity: 1,
                appearance: String::new(),
                identified: true,
                description: String::new(),
                id,
                name: String::new(),
                asset: None,
            },
            position: Position { x, y, z },
        }
    }

    fn door(open: bool) -> DoorView {
        DoorView {
            id: 1,
            name: "door".into(),
            description: String::new(),
            open,
            reachable: true,
            approaches: Vec::new(),
            asset: None,
        }
    }

    fn glyph(
        observation: &Observation,
        chart: &[RememberedCell],
        x: i32,
        y: i32,
    ) -> Option<ColumnGlyph> {
        let refs: Vec<_> = chart.iter().collect();
        column_glyph(observation, &refs, x, y)
    }

    fn remembered(cell: CellView) -> RememberedCell {
        RememberedCell {
            door: cell.door,
            material: cell.material,
            key: cell.key,
            position: cell.position,
            wall: cell.wall,
            place_hint: cell.place_hint,
            last_seen_tick: 0,
            last_seen_revision: 0,
            ground_items: Vec::new(),
            visible_actors: Vec::new(),
            stairs_up: cell.stairs_up,
            stairs_down: cell.stairs_down,
        }
    }

    #[test]
    fn worked_columns_follow_standing_plane_precedence() {
        let mut reference = view_at(0);
        reference.ground_items.push(item(3, 0, 0, 0));
        reference.visible_actors.push(actor(2, 0, 0, 1));
        reference.visible_cells.push(cell("feet", 0, 0, 0, false));
        let here = glyph(&reference, &[], 0, 0).unwrap();
        assert_eq!(here.ch, '@');
        assert_eq!(here.color, 0xFFFFFF);
        assert!(!here.remembered);

        let mut body = view_at(0);
        body.visible_cells.push(cell("floor", 2, 0, 0, false));
        body.visible_actors.push(actor(1, 2, 0, 1));
        assert_eq!(glyph(&body, &[], 2, 0).unwrap().ch, '.');

        let mut open_door = view_at(0);
        let mut door_cell = cell("door", 1, 0, 0, false);
        door_cell.door = Some(door(true));
        door_cell.stairs_up = true;
        open_door.visible_cells.push(door_cell);
        assert_eq!(glyph(&open_door, &[], 1, 0).unwrap().ch, '/');

        let mut closed = view_at(0);
        let mut shut = cell("shut", 1, 0, 0, false);
        shut.door = Some(door(false));
        closed.visible_cells.push(shut);
        closed.visible_actors.push(actor(4, 1, 0, 2));
        assert_eq!(glyph(&closed, &[], 1, 0).unwrap().ch, '+');

        let mut both = view_at(0);
        let mut stairs = cell("stairs", 1, 0, 0, false);
        stairs.stairs_up = true;
        stairs.stairs_down = true;
        stairs.asset = Some("terrain.floor.stone".into());
        both.visible_cells.push(stairs);
        assert_eq!(glyph(&both, &[], 1, 0).unwrap().ch, '<');

        let mut split = view_at(0);
        let mut up = cell("up", 1, 0, 0, false);
        up.stairs_up = true;
        up.asset = Some("terrain.floor.stone".into());
        let mut down = cell("down", 1, 0, 3, false);
        down.stairs_down = true;
        split.visible_cells.extend([up, down.clone()]);
        assert_eq!(glyph(&split, &[], 1, 0).unwrap().ch, '<');
        let mut plain = view_at(0);
        plain.visible_cells.push(cell("plain", 1, 0, 0, false));
        plain.visible_cells.push(down);
        assert_eq!(glyph(&plain, &[], 1, 0).unwrap().ch, '.');

        let mut walled = view_at(0);
        walled.visible_cells.push(cell("wall", 1, 0, 0, true));
        walled.visible_actors.push(actor(5, 1, 0, 2));
        assert_eq!(glyph(&walled, &[], 1, 0).unwrap().ch, '#');

        let mut pit = view_at(0);
        let mut hole = cell("hole", 1, 0, 0, false);
        hole.asset = Some("terrain.floor.stone".into());
        pit.visible_cells
            .extend([hole, cell("under", 1, 0, -1, false)]);
        pit.visible_actors.push(actor(6, 1, 0, 2));
        assert_eq!(glyph(&pit, &[], 1, 0).unwrap().ch, '&');
        pit.visible_actors.clear();
        pit.ground_items.push(item(8, 1, 0, 4));
        assert_eq!(glyph(&pit, &[], 1, 0).unwrap().ch, '!');
        pit.ground_items.clear();
        let caret = glyph(&pit, &[], 1, 0).unwrap();
        assert_eq!(caret.ch, '^');
        assert!(!caret.remembered);
        assert_eq!(caret.color, 0xD07A4A);

        let wall = remembered(cell("old-wall", 4, 0, 0, true));
        let grey = glyph(&view_at(0), &[wall], 4, 0).unwrap();
        assert_eq!(grey.ch, '#');
        assert!(grey.remembered);
        assert_eq!(grey.color, REMEMBERED_COLOR);
        assert!(glyph(&view_at(0), &[], 9, 9).is_none());
    }

    #[test]
    fn pits_need_a_seen_open_cell_under_a_seen_open_standing_cell() {
        let mut open = view_at(0);
        open.visible_cells.push(cell("floor", 1, 0, 0, false));
        assert_eq!(glyph(&open, &[], 1, 0).unwrap().ch, '.');
        open.visible_cells.push(cell("solid", 1, 0, -1, true));
        assert_eq!(glyph(&open, &[], 1, 0).unwrap().ch, '.');
        open.visible_cells.pop();
        let mut shut = cell("lid", 1, 0, -1, false);
        shut.door = Some(door(false));
        open.visible_cells.push(shut);
        assert_eq!(glyph(&open, &[], 1, 0).unwrap().ch, '.');

        let mut high = view_at(i32::MIN);
        high.visible_cells.push(cell("edge", 1, 0, i32::MIN, false));
        high.visible_cells
            .push(cell("wrapped", 1, 0, i32::MAX, false));
        assert_eq!(glyph(&high, &[], 1, 0).unwrap().ch, '.');
    }

    #[test]
    fn a_current_hole_over_a_remembered_open_cell_is_a_current_pit() {
        let mut current = view_at(0);
        current.visible_cells.push(cell("now", 1, 0, 0, false));
        let lower = remembered(cell("then", 1, 0, -1, false));
        let pit = glyph(&current, std::slice::from_ref(&lower), 1, 0).unwrap();
        assert_eq!(pit.ch, '^');
        assert!(!pit.remembered);
        assert_ne!(pit.color, REMEMBERED_COLOR);

        let standing = remembered(cell("old-stand", 1, 0, 0, false));
        let grey = glyph(&view_at(0), &[standing, lower], 1, 0).unwrap();
        assert_eq!(grey.ch, '^');
        assert!(grey.remembered);
        assert_eq!(grey.color, REMEMBERED_COLOR);
    }

    #[test]
    fn two_cells_at_one_coordinate_use_existential_order() {
        let open = cell("open", 1, 0, 0, false);
        let mut wall = cell("wall", 1, 0, 0, true);
        wall.key = "wall".into();
        for cells in [vec![open.clone(), wall.clone()], vec![wall, open]] {
            let mut view = view_at(0);
            view.visible_cells = cells;
            assert_eq!(glyph(&view, &[], 1, 0).unwrap().ch, '#');
        }
    }

    #[test]
    fn an_off_plane_solid_does_not_draw_and_the_lowest_ids_win() {
        let mut view = view_at(0);
        view.visible_cells.push(cell("below", 1, 0, -3, true));
        assert!(glyph(&view, &[], 1, 0).is_none());
        view.visible_actors
            .extend([actor(9, 2, 0, 1), actor(3, 2, 0, 4)]);
        view.ground_items
            .extend([item(8, 3, 0, 1), item(2, 3, 0, 0)]);
        view.visible_cells.push(cell("a", 2, 0, 0, false));
        view.visible_cells.push(cell("b", 3, 0, 0, false));
        assert_eq!(glyph(&view, &[], 2, 0).unwrap().ch, '&');
        assert_eq!(glyph(&view, &[], 3, 0).unwrap().ch, '!');
    }
}
