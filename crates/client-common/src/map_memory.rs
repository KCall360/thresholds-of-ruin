//! Bounded appearance storage and a chart aligned only by disclosed identities.
use crate::RememberedCell;
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::*;

/// A chart occurrence borrows its appearance; several offsets may name one cell.
#[derive(Clone, Copy, Debug)]
pub struct MapCell<'a> {
    pub position: Position,
    pub cell: &'a RememberedCell,
}
impl std::ops::Deref for MapCell<'_> {
    type Target = RememberedCell;
    fn deref(&self) -> &Self::Target {
        self.cell
    }
}
impl<'a> MapCell<'a> {
    pub fn appearance(&self) -> RememberedMapCell {
        RememberedMapCell {
            position: self.position,
            ..self.cell.appearance.clone()
        }
    }
    pub fn items(&self) -> &'a [MapItemAppearance] {
        &self.cell.ground_items
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct MapMemory {
    pub records: BTreeMap<String, RememberedCell>,
    pub cells: BTreeMap<Position, String>,
    previous: BTreeMap<String, Vec<Position>>,
    record_sizes: BTreeMap<String, usize>,
    bytes: usize,
    pub limited: bool,
    pub coverage: Option<MapWindow>,
}

pub(crate) fn appearance(cell: &CellView, items: &[&GroundItemView]) -> RememberedMapCell {
    RememberedMapCell {
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
            .iter()
            .map(|g| MapItemAppearance {
                class: g.item.class,
                description: g.item.description.clone(),
                name: g.item.name.clone(),
                appearance: g.item.appearance.clone(),
                identified: g.item.identified,
                quantity: g.item.quantity,
                asset: g.item.asset.clone(),
            })
            .collect(),
    }
}

impl MapMemory {
    pub fn positioned(&self) -> impl Iterator<Item = MapCell<'_>> {
        self.cells.iter().filter_map(|(position, key)| {
            self.records.get(key).map(|cell| MapCell {
                position: *position,
                cell,
            })
        })
    }
    pub fn at(&self, position: Position) -> Option<RememberedMapCell> {
        self.records
            .get(self.cells.get(&position)?)
            .map(|c| RememberedMapCell {
                position,
                ..c.appearance.clone()
            })
    }
    /// Translation uses every unique common anchor. No anchor or disagreement
    /// discards only placement, never creates a guessed embedding.
    pub fn observe(&mut self, view: &Observation, revision: u64) -> bool {
        let mut current: BTreeMap<String, Vec<Position>> = BTreeMap::new();
        for cell in &view.visible_cells {
            current
                .entry(cell.key.clone())
                .or_default()
                .push(cell.position);
        }
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
                    conflict |= shift.is_some_and(|s| s != delta);
                    shift = Some(delta);
                }
            }
        }
        let translation = shift.filter(|_| !conflict);
        if translation != Some((0, 0, 0)) {
            let old = std::mem::take(&mut self.cells);
            self.coverage = self.coverage.and_then(|w| {
                let (dx, dy, dz) = translation?;
                Some(MapWindow {
                    min: translate(w.min, dx, dy, dz)?,
                    max: translate(w.max, dx, dy, dz)?,
                })
            });
            if let Some((dx, dy, dz)) = translation {
                for (p, key) in old {
                    if let Some(p) = translate(p, dx, dy, dz) {
                        self.cells.insert(p, key);
                    }
                }
            }
        }
        let window = MapWindow::normal().checked_translate(view.position);
        // The whole normal footprint is retained, at all six height planes.
        self.cells
            .retain(|p, _| window.as_ref().is_some_and(|w| w.contains(*p)));
        self.coverage = self.coverage.and_then(|old| {
            let window = window?;
            let clipped = MapWindow {
                min: Position {
                    x: old.min.x.max(window.min.x),
                    y: old.min.y.max(window.min.y),
                    z: old.min.z.max(window.min.z),
                },
                max: Position {
                    x: old.max.x.min(window.max.x),
                    y: old.max.y.min(window.max.y),
                    z: old.max.z.min(window.max.z),
                },
            };
            clipped.validate().ok().map(|_| clipped)
        });
        self.overlay(view, revision);
        self.previous = current;
        translation != Some((0, 0, 0))
    }
    pub fn overlay(&mut self, view: &Observation, revision: u64) {
        let window = MapWindow::normal().checked_translate(view.position);
        let mut items: BTreeMap<Position, Vec<&GroundItemView>> = BTreeMap::new();
        for item in &view.ground_items {
            items.entry(item.position).or_default().push(item);
        }
        for cell in &view.visible_cells {
            let known = RememberedCell {
                appearance: appearance(cell, items.get(&cell.position).map_or(&[], Vec::as_slice)),
                last_seen_tick: Some(view.tick),
                last_seen_revision: Some(revision),
            };
            self.put(cell.key.clone(), known);
            if self.records.contains_key(&cell.key)
                && window.as_ref().is_some_and(|w| w.contains(cell.position))
            {
                self.cells.insert(cell.position, cell.key.clone());
            }
        }
        self.trim();
    }
    fn remove(&mut self, key: &str) {
        self.records.remove(key);
        self.bytes -= self.record_sizes.remove(key).unwrap_or(0);
    }
    fn put(&mut self, key: String, cell: RememberedCell) {
        if let Some(old) = self.records.get_mut(&key) {
            if old.appearance == cell.appearance {
                old.last_seen_tick = cell.last_seen_tick;
                old.last_seen_revision = cell.last_seen_revision;
                return;
            }
        }
        let size = encoded_length(&cell.appearance, MAX_MAP_TRANSFER_BYTES)
            .ok()
            .flatten();
        self.remove(&key);
        if let Some(size) = size {
            self.bytes += size;
            self.record_sizes.insert(key.clone(), size);
            self.records.insert(key, cell);
        } else {
            self.cells.retain(|_, k| k != &key);
            self.limited = true;
            self.coverage = None;
        }
    }
    fn trim(&mut self) {
        let placed: BTreeSet<_> = self.cells.values().cloned().collect();
        if self.records.len() > MAX_MAP_TRANSFER_CELLS || self.bytes > MAX_MAP_TRANSFER_BYTES {
            let keys: Vec<_> = self
                .records
                .keys()
                .filter(|key| !placed.contains(*key))
                .cloned()
                .collect();
            for key in keys {
                if self.records.len() <= MAX_MAP_TRANSFER_CELLS
                    && self.bytes <= MAX_MAP_TRANSFER_BYTES
                {
                    break;
                }
                self.remove(&key);
            }
        }
        // Pathological valid live metadata may exceed the independent cache
        // budget. Its current Observation still renders; no silent claim of
        // complete stale coverage survives an eviction.
        while self.records.len() > MAX_MAP_TRANSFER_CELLS || self.bytes > MAX_MAP_TRANSFER_BYTES {
            let Some(key) = self.records.last_key_value().map(|(k, _)| k.clone()) else {
                break;
            };
            self.remove(&key);
            self.limited = true;
            self.coverage = None;
        }
        self.cells.retain(|_, key| self.records.contains_key(key));
    }
    pub fn needs_window(&self, requested: MapWindow) -> bool {
        self.coverage
            .as_ref()
            .is_none_or(|w| !w.contains(requested.min) || !w.contains(requested.max))
    }
    pub fn install(&mut self, window: MapWindow, cells: BTreeMap<Position, RememberedMapCell>) {
        self.cells.clear();
        self.limited = false;
        for (position, appearance) in cells {
            let key = appearance.key.clone();
            // Existing exact last sightings retain their honest diagnostic metadata.
            let keep = self.records.get(&key).is_some_and(|old| {
                let mut same = old.appearance.clone();
                same.position = position;
                same == appearance
            });
            if !keep {
                self.put(
                    key.clone(),
                    RememberedCell {
                        appearance,
                        last_seen_tick: None,
                        last_seen_revision: None,
                    },
                );
            }
            self.cells.insert(position, key);
        }
        self.coverage = Some(window);
        self.trim();
    }
}
fn translate(p: Position, x: i64, y: i64, z: i64) -> Option<Position> {
    Some(Position {
        x: i32::try_from(i64::from(p.x).checked_add(x)?).ok()?,
        y: i32::try_from(i64::from(p.y).checked_add(y)?).ok()?,
        z: i32::try_from(i64::from(p.z).checked_add(z)?).ok()?,
    })
}
