//! A local chart aligned only by overlapping disclosed cell identities.
use crate::RememberedCell;
use std::collections::BTreeMap;
use tor_protocol::{Observation, Position};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MapMemory {
    pub cells: BTreeMap<(i32, i32, i32), RememberedCell>,
    previous: BTreeMap<String, Vec<Position>>,
}

impl MapMemory {
    pub fn observe(&mut self, view: &Observation, memory: &BTreeMap<String, RememberedCell>) {
        let mut current: BTreeMap<String, Vec<Position>> = BTreeMap::new();
        for cell in &view.visible_cells {
            current
                .entry(cell.key.clone())
                .or_default()
                .push(cell.position);
        }
        // A uniquely seen common cell anchors a translation. Every such anchor
        // must agree; rotated/cyclic charts need not admit a global embedding.
        let mut shift = None;
        let mut conflict = false;
        for (key, positions) in &current {
            if let Some(old) = self.previous.get(key) {
                if let ([before], [after]) = (old.as_slice(), positions.as_slice()) {
                    let delta = (
                        i64::from(after.x) - i64::from(before.x),
                        i64::from(after.y) - i64::from(before.y),
                        i64::from(after.z) - i64::from(before.z),
                    );
                    if shift.is_some_and(|s| s != delta) {
                        conflict = true;
                    }
                    shift = Some(delta);
                }
            }
        }
        let translation = shift.filter(|_| !conflict);
        if translation != Some((0, 0, 0)) {
            let old_cells = std::mem::take(&mut self.cells);
            if let Some((dx, dy, dz)) = translation {
                for mut cell in old_cells.into_values() {
                    let (Ok(x), Ok(y), Ok(z)) = (
                        i32::try_from(i64::from(cell.position.x) + dx),
                        i32::try_from(i64::from(cell.position.y) + dy),
                        i32::try_from(i64::from(cell.position.z) + dz),
                    ) else {
                        continue;
                    };
                    cell.position = Position { x, y, z };
                    for item in &mut cell.ground_items {
                        item.position = cell.position;
                    }
                    self.cells.insert((x, y, z), cell);
                }
            }
        }
        // Seeing a physical cell refreshes all of its retained occurrences.
        // Occupants are never carried into the remembered map.
        for cell in self.cells.values_mut() {
            if current.contains_key(&cell.key) {
                let position = cell.position;
                *cell = memory[&cell.key].clone();
                cell.position = position;
                cell.visible_actors.clear();
                for item in &mut cell.ground_items {
                    item.position = position;
                }
            }
        }
        for cell in &view.visible_cells {
            let mut remembered = memory[&cell.key].clone();
            remembered.position = cell.position;
            remembered.visible_actors.clear();
            for item in &mut remembered.ground_items {
                item.position = cell.position;
            }
            self.cells.insert(
                (cell.position.x, cell.position.y, cell.position.z),
                remembered,
            );
        }
        // Bound this presentation cache without changing historical memory.
        if self.cells.len() > 4096 {
            let mut positions: Vec<_> = self.cells.keys().copied().collect();
            positions.sort_by_key(|&(x, y, z)| {
                (
                    i64::from(x).abs() + i64::from(y).abs() + i64::from(z).abs(),
                    x,
                    y,
                    z,
                )
            });
            for position in positions.into_iter().skip(4096) {
                self.cells.remove(&position);
            }
        }
        self.previous = current;
    }
}
