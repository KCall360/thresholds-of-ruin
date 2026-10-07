//! View deltas: an observation expressed as changes to the previous one on the
//! same stream. Applying a delta to its base reproduces the full observation.

use crate::wire::*;
use crate::ActorId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Changes to the visible cells since the base observation.
///
/// Cell positions are observer-relative, so a step moves every retained cell.
/// `shift` is added to each retained position first; `removed` and `changed`
/// then use the new frame. Cells that did not move with the shift, such as
/// cells seen through a rotated link, are removed and sent again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellChanges {
    pub shift: Position,
    pub removed: Vec<Position>,
    /// Cells entering view or differing from the shifted base cell.
    pub changed: Vec<CellView>,
}

/// An ordered edit against the original base collection. Ranges never refer to
/// the partially edited result. Empty edit lists retain the whole collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionEdit<T> {
    pub start: u32,
    pub remove: u32,
    pub insert: Vec<T>,
}

/// A state view expressed as cell changes and exact ordered collection edits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateDelta {
    /// Revision of the state this delta applies to.
    #[serde(with = "crate::integers::unsigned")]
    pub base_revision: u64,
    pub wizard_game: bool,
    #[serde(with = "crate::integers::unsigned")]
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionView>,
    pub places: Vec<CollectionEdit<PlaceView>>,
    pub actor: ActorId,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub position: Position,
    pub cells: CellChanges,
    pub ground_items: Vec<CollectionEdit<GroundItemView>>,
    pub inventory: Vec<CollectionEdit<ItemView>>,
    pub visible_actors: Vec<CollectionEdit<ActorView>>,
    pub ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaError {
    /// The delta was computed against a different state.
    WrongBase,
    /// A change does not fit the base, or its translation overflows.
    InvalidChange,
}

fn add(a: Position, b: Position) -> Option<Position> {
    Some(Position {
        x: a.x.checked_add(b.x)?,
        y: a.y.checked_add(b.y)?,
        z: a.z.checked_add(b.z)?,
    })
}

fn subtract(a: Position, b: Position) -> Option<Position> {
    Some(Position {
        x: a.x.checked_sub(b.x)?,
        y: a.y.checked_sub(b.y)?,
        z: a.z.checked_sub(b.z)?,
    })
}

fn sorted(cells: &[CellView]) -> bool {
    cells
        .windows(2)
        .all(|pair| pair[0].position < pair[1].position)
}

/// The translation that keeps the most cells in place, found by matching
/// cells whose key appears exactly once in both views. Ties choose the
/// smallest translation so the result is deterministic.
fn best_shift(base: &[CellView], next: &[CellView]) -> Position {
    let mut positions: HashMap<&str, Option<Position>> = HashMap::with_capacity(base.len());
    for cell in base {
        positions
            .entry(&cell.key)
            .and_modify(|seen| *seen = None)
            .or_insert(Some(cell.position));
    }
    let mut repeated: HashMap<&str, bool> = HashMap::with_capacity(next.len());
    for cell in next {
        repeated
            .entry(&cell.key)
            .and_modify(|seen| *seen = true)
            .or_insert(false);
    }
    let mut votes: BTreeMap<Position, usize> = BTreeMap::new();
    for cell in next {
        if repeated[cell.key.as_str()] {
            continue;
        }
        if let Some(Some(from)) = positions.get(cell.key.as_str()) {
            let Some(shift) = subtract(cell.position, *from) else {
                continue;
            };
            *votes.entry(shift).or_default() += 1;
        }
    }
    let mut best = (0, Position { x: 0, y: 0, z: 0 });
    for (shift, count) in votes {
        if count > best.0 {
            best = (count, shift);
        }
    }
    best.1
}

impl StateDelta {
    /// Changes from `base` to `next`, or `None` when a delta cannot represent
    /// `next` exactly. Encoded-message size selection is a separate boundary.
    pub fn between(base: &StateView, next: &StateView) -> Option<Self> {
        let old = &base.observation.visible_cells;
        let new = &next.observation.visible_cells;
        if !sorted(old) || !sorted(new) {
            return None;
        }
        let shift = best_shift(old, new);
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        let (mut i, mut j) = (0, 0);
        while i < old.len() || j < new.len() {
            let before = match old.get(i) {
                Some(cell) => Some(add(cell.position, shift)?),
                None => None,
            };
            let after = new.get(j).map(|cell| cell.position);
            match (before, after) {
                (Some(b), Some(a)) if b == a => {
                    let (was, is) = (&old[i], &new[j]);
                    // Exhaustive so that a new cell field cannot be missed.
                    let CellView {
                        door,
                        material,
                        key,
                        stairs_up,
                        stairs_down,
                        position: _,
                        wall,
                        place_hint,
                        asset,
                    } = was;
                    if *key != is.key
                        || *asset != is.asset
                        || *door != is.door
                        || *material != is.material
                        || *stairs_up != is.stairs_up
                        || *stairs_down != is.stairs_down
                        || *wall != is.wall
                        || *place_hint != is.place_hint
                    {
                        changed.push(is.clone());
                    }
                    i += 1;
                    j += 1;
                }
                (Some(b), Some(a)) if b < a => {
                    removed.push(b);
                    i += 1;
                }
                (Some(b), None) => {
                    removed.push(b);
                    i += 1;
                }
                _ => {
                    changed.push(new[j].clone());
                    j += 1;
                }
            }
        }
        // Shifting preserves order, so the merge above saw both lists sorted.
        let observation = &next.observation;
        Some(Self {
            base_revision: base.revision,
            wizard_game: next.wizard_game,
            revision: next.revision,
            combat: observation.combat.clone(),
            motion: observation.motion.clone(),
            places: collection_changes(&base.observation.places, &observation.places, |place| {
                place.key.as_str()
            })?,
            actor: observation.actor,
            tick: observation.tick,
            position: observation.position,
            cells: CellChanges {
                shift,
                removed,
                changed,
            },
            ground_items: collection_changes(
                &base.observation.ground_items,
                &observation.ground_items,
                |item| (item.position, item.item.id),
            )?,
            inventory: collection_changes(
                &base.observation.inventory,
                &observation.inventory,
                |item| item.id,
            )?,
            visible_actors: collection_changes(
                &base.observation.visible_actors,
                &observation.visible_actors,
                |actor| (actor.position, actor.id),
            )?,
            ready: observation.ready,
        })
    }

    /// The full state this delta describes. Fails without partial effects.
    pub fn apply(self, base: &StateView) -> Result<StateView, DeltaError> {
        if base.revision != self.base_revision {
            return Err(DeltaError::WrongBase);
        }
        let CellChanges {
            shift,
            removed,
            changed,
        } = self.cells;
        if !sorted(&base.observation.visible_cells)
            || !removed.windows(2).all(|pair| pair[0] < pair[1])
            || !sorted(&changed)
        {
            return Err(DeltaError::InvalidChange);
        }
        let old = &base.observation.visible_cells;
        let mut cells = Vec::with_capacity(old.len() + changed.len());
        let mut removed = removed.into_iter().peekable();
        let mut changed = changed.into_iter().peekable();
        for cell in old {
            let position = add(cell.position, shift).ok_or(DeltaError::InvalidChange)?;
            while let Some(next) = changed.next_if(|next| next.position < position) {
                cells.push(next);
            }
            if removed.next_if_eq(&position).is_some() {
                if changed.peek().is_some_and(|next| next.position == position) {
                    return Err(DeltaError::InvalidChange);
                }
                continue;
            }
            if let Some(next) = changed.next_if(|next| next.position == position) {
                cells.push(next);
            } else {
                let mut cell = cell.clone();
                cell.position = position;
                cells.push(cell);
            }
        }
        cells.extend(changed);
        if removed.next().is_some() || !sorted(&cells) {
            return Err(DeltaError::InvalidChange);
        }
        Ok(StateView {
            wizard_game: self.wizard_game,
            revision: self.revision,
            observation: Observation {
                combat: self.combat,
                motion: self.motion,
                places: apply_collection_changes(&base.observation.places, self.places)?,
                actor: self.actor,
                tick: self.tick,
                position: self.position,
                visible_cells: cells,
                ground_items: apply_collection_changes(
                    &base.observation.ground_items,
                    self.ground_items,
                )?,
                inventory: apply_collection_changes(&base.observation.inventory, self.inventory)?,
                visible_actors: apply_collection_changes(
                    &base.observation.visible_actors,
                    self.visible_actors,
                )?,
                ready: self.ready,
            },
        })
    }
}

// Retain equal occurrences in forward order. This is deliberately not an
// optimal general-purpose diff: exact reconstruction and bounded work matter
// more than minimizing edit count. The complete envelope encoder chooses size.
fn collection_changes<'a, T: Clone + Eq, K: Ord>(
    base: &'a [T],
    next: &'a [T],
    key: impl Fn(&'a T) -> K,
) -> Option<Vec<CollectionEdit<T>>> {
    if base == next {
        return Some(Vec::new());
    }
    let mut positions = BTreeMap::new();
    for (index, value) in base.iter().enumerate() {
        if positions.insert(key(value), index).is_some() {
            return None;
        }
    }
    let mut edits = Vec::new();
    let (mut old_start, mut new_start) = (0, 0);
    for (new_index, value) in next.iter().enumerate() {
        let Some(&old_index) = positions.get(&key(value)) else {
            continue;
        };
        if old_index < old_start || base[old_index] != *value {
            continue;
        }
        if old_index != old_start || new_index != new_start {
            edits.push(CollectionEdit {
                start: u32::try_from(old_start).ok()?,
                remove: u32::try_from(old_index - old_start).ok()?,
                insert: next[new_start..new_index].to_vec(),
            });
        }
        old_start = old_index + 1;
        new_start = new_index + 1;
    }
    if old_start != base.len() || new_start != next.len() {
        edits.push(CollectionEdit {
            start: u32::try_from(old_start).ok()?,
            remove: u32::try_from(base.len() - old_start).ok()?,
            insert: next[new_start..].to_vec(),
        });
    }
    Some(edits)
}

fn apply_collection_changes<T: Clone>(
    base: &[T],
    edits: Vec<CollectionEdit<T>>,
) -> Result<Vec<T>, DeltaError> {
    // Validate every original-base range and total output length before copying.
    let mut length = base.len();
    let mut previous = None;
    for edit in &edits {
        let start = usize::try_from(edit.start).map_err(|_| DeltaError::InvalidChange)?;
        let remove = usize::try_from(edit.remove).map_err(|_| DeltaError::InvalidChange)?;
        let end = start.checked_add(remove).ok_or(DeltaError::InvalidChange)?;
        if end > base.len()
            || (remove == 0 && edit.insert.is_empty())
            || previous.is_some_and(|(before, before_end)| start <= before || start < before_end)
        {
            return Err(DeltaError::InvalidChange);
        }
        length = length
            .checked_sub(remove)
            .and_then(|n| n.checked_add(edit.insert.len()))
            .ok_or(DeltaError::InvalidChange)?;
        previous = Some((start, end));
    }
    let mut result = Vec::new();
    result
        .try_reserve_exact(length)
        .map_err(|_| DeltaError::InvalidChange)?;
    let mut cursor = 0;
    for edit in edits {
        let start = edit.start as usize;
        result.extend_from_slice(&base[cursor..start]);
        cursor = start + edit.remove as usize;
        result.extend(edit.insert);
    }
    result.extend_from_slice(&base[cursor..]);
    Ok(result)
}
