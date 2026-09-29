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

/// A state view whose visible cells are sent as changes. Every other field is
/// sent in full; together they are small next to the cells.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateDelta {
    /// Revision of the state this delta applies to.
    pub base_revision: u64,
    pub wizard_game: bool,
    pub revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<MotionView>,
    pub places: Vec<PlaceView>,
    pub actor: ActorId,
    pub tick: u64,
    pub position: Position,
    pub cells: CellChanges,
    pub ground_items: Vec<GroundItemView>,
    pub inventory: Vec<ItemView>,
    pub visible_actors: Vec<ActorView>,
    pub ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaError {
    /// The delta was computed against a different state.
    WrongBase,
    /// A removed or changed cell does not fit the shifted base.
    InvalidChange,
}

fn add(a: Position, b: Position) -> Position {
    Position {
        x: a.x.wrapping_add(b.x),
        y: a.y.wrapping_add(b.y),
        z: a.z.wrapping_add(b.z),
    }
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
            let shift = Position {
                x: cell.position.x.wrapping_sub(from.x),
                y: cell.position.y.wrapping_sub(from.y),
                z: cell.position.z.wrapping_sub(from.z),
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
    /// `next` exactly or would not be smaller than sending it in full.
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
            let before = old.get(i).map(|cell| add(cell.position, shift));
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
                    } = was;
                    if *key != is.key
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
        if changed.len() + removed.len() / 4 >= new.len() {
            return None;
        }
        let observation = &next.observation;
        Some(Self {
            base_revision: base.revision,
            wizard_game: next.wizard_game,
            revision: next.revision,
            combat: observation.combat.clone(),
            motion: observation.motion.clone(),
            places: observation.places.clone(),
            actor: observation.actor,
            tick: observation.tick,
            position: observation.position,
            cells: CellChanges {
                shift,
                removed,
                changed,
            },
            ground_items: observation.ground_items.clone(),
            inventory: observation.inventory.clone(),
            visible_actors: observation.visible_actors.clone(),
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
            let position = add(cell.position, shift);
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
                places: self.places,
                actor: self.actor,
                tick: self.tick,
                position: self.position,
                visible_cells: cells,
                ground_items: self.ground_items,
                inventory: self.inventory,
                visible_actors: self.visible_actors,
                ready: self.ready,
            },
        })
    }
}
