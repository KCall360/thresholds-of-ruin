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
        object_presentation: cell.object_presentation.clone(),
        key: cell.key.clone(),
        position: cell.position,
        wall: cell.wall,
        material: cell.material.clone(),
        asset: cell.asset.clone(),
        place_hint: cell.place_hint,
        stairs_up: cell.stairs_up,
        stairs_down: cell.stairs_down,
        trap: cell.trap.as_ref().map(|trap| MapTrapAppearance {
            family: trap.family,
        }),
        door: cell.door.as_ref().map(|d| MapDoorAppearance {
            name: d.name.clone(),
            description: d.description.clone(),
            open: d.open,
            asset: d.asset.clone(),
        }),
        ground_items: items
            .iter()
            .map(|g| MapItemAppearance {
                presentation: g.presentation.clone(),
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
        let mut anchors = Vec::new();
        let mut shift = None;
        let mut conflict = false;
        for (key, positions) in &current {
            if let Some(old) = self.previous.get(key) {
                if let ([before], [after]) = (old.as_slice(), positions.as_slice()) {
                    anchors.push(*before);
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
        // A translation of one point or a line cannot exclude a rotation.
        // Only two independent anchor vectors certify retained coverage.
        let oriented = anchors.first().is_some_and(|origin| {
            let vector = |a: &Position| {
                [
                    i128::from(a.x) - i128::from(origin.x),
                    i128::from(a.y) - i128::from(origin.y),
                    i128::from(a.z) - i128::from(origin.z),
                ]
            };
            anchors
                .iter()
                .skip(1)
                .map(vector)
                .find(|v| *v != [0; 3])
                .is_some_and(|v| {
                    anchors.iter().skip(1).map(vector).any(|w| {
                        v[0] * w[1] != v[1] * w[0]
                            || v[0] * w[2] != v[2] * w[0]
                            || v[1] * w[2] != v[2] * w[1]
                    })
                })
        });
        let lost_orientation = self.coverage.is_some() && !oriented;
        if !oriented {
            self.coverage = None;
        }
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
        translation != Some((0, 0, 0)) || lost_orientation
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
    /// Grow the certified rectangle by an entire adjacent face, never a
    /// bounding box over holes. Later axes include already committed corners.
    pub fn next_uncovered(&self, requested: MapWindow) -> Option<MapWindow> {
        let Some(covered) = self.coverage else {
            return Some(requested);
        };
        let mut min = [covered.min.x, covered.min.y, covered.min.z];
        let mut max = [covered.max.x, covered.max.y, covered.max.z];
        let wanted_min = [requested.min.x, requested.min.y, requested.min.z];
        let wanted_max = [requested.max.x, requested.max.y, requested.max.z];
        for axis in 0..3 {
            if min[axis] > wanted_min[axis] {
                max[axis] = min[axis].checked_sub(1)?;
                min[axis] = wanted_min[axis];
                return Some(bounds(min, max));
            }
            if max[axis] < wanted_max[axis] {
                min[axis] = max[axis].checked_add(1)?;
                max[axis] = wanted_max[axis];
                return Some(bounds(min, max));
            }
        }
        None
    }
    pub fn install_region(
        &mut self,
        viewport: MapWindow,
        window: MapWindow,
        cells: BTreeMap<Position, RememberedMapCell>,
    ) -> Result<(), &'static str> {
        if self.next_uncovered(viewport) != Some(window) {
            return Err("Map region is not the next exact uncovered face");
        }
        let coverage = self.coverage.map_or(window, |old| MapWindow {
            min: Position {
                x: old.min.x.min(window.min.x),
                y: old.min.y.min(window.min.y),
                z: old.min.z.min(window.min.z),
            },
            max: Position {
                x: old.max.x.max(window.max.x),
                y: old.max.y.max(window.max.y),
                z: old.max.z.max(window.max.z),
            },
        });
        self.cells.retain(|position, _| !window.contains(*position));
        self.coverage = Some(coverage);
        self.install_cells(cells);
        self.trim();
        Ok(())
    }
    /// Native presentation receives a complete positioned snapshot, including
    /// honest partial coverage; it never receives unfinished transfer staging.
    pub fn install_snapshot(
        &mut self,
        coverage: Option<MapWindow>,
        cells: BTreeMap<Position, RememberedMapCell>,
    ) {
        self.cells.clear();
        self.coverage = coverage;
        self.install_cells(cells);
        self.trim();
    }
    fn install_cells(&mut self, cells: BTreeMap<Position, RememberedMapCell>) {
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
    }
}
fn translate(p: Position, x: i64, y: i64, z: i64) -> Option<Position> {
    Some(Position {
        x: i32::try_from(i64::from(p.x).checked_add(x)?).ok()?,
        y: i32::try_from(i64::from(p.y).checked_add(y)?).ok()?,
        z: i32::try_from(i64::from(p.z).checked_add(z)?).ok()?,
    })
}

fn bounds(min: [i32; 3], max: [i32; 3]) -> MapWindow {
    MapWindow {
        min: Position {
            x: min[0],
            y: min[1],
            z: min[2],
        },
        max: Position {
            x: max[0],
            y: max[1],
            z: max[2],
        },
    }
}
