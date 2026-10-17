//! Descriptive memory beside the live observation. No remembered action handles.
use std::collections::{BTreeMap, BTreeSet};
use tor_client_common::ClientState;
use tor_protocol::{CellView, MapDoorAppearance, Position, StateView};

/// Presentation geometry can include stale cells; actions always use `current`.
pub trait ScenicView: Sync {
    fn current(&self) -> &StateView;
    fn scenery(&self) -> &StateView;
    fn remembered_door(&self, _position: Position) -> Option<&MapDoorAppearance> {
        None
    }
}
impl ScenicView for StateView {
    fn current(&self) -> &StateView {
        self
    }
    fn scenery(&self) -> &StateView {
        self
    }
}
impl<T: ScenicView + ?Sized> ScenicView for &T {
    fn current(&self) -> &StateView {
        T::current(*self)
    }
    fn scenery(&self) -> &StateView {
        T::scenery(*self)
    }
    fn remembered_door(&self, position: Position) -> Option<&MapDoorAppearance> {
        T::remembered_door(*self, position)
    }
}
impl ScenicView for std::sync::Arc<StateView> {
    fn current(&self) -> &StateView {
        self
    }
    fn scenery(&self) -> &StateView {
        self
    }
}

/// Owned so a pre-travel description survives the next live observation.
pub struct Seen {
    current: StateView,
    scenery: StateView,
    doors: BTreeMap<Position, MapDoorAppearance>,
}
impl std::ops::Deref for Seen {
    type Target = StateView;
    fn deref(&self) -> &Self::Target {
        &self.current
    }
}
impl ScenicView for Seen {
    fn current(&self) -> &StateView {
        &self.current
    }
    fn scenery(&self) -> &StateView {
        &self.scenery
    }
    fn remembered_door(&self, position: Position) -> Option<&MapDoorAppearance> {
        self.doors.get(&position)
    }
}

/// Fill partially visible columns and nearby cells from aligned visual memory.
/// Live appearances win, and all live item/actor/door capabilities stay untouched.
pub fn seen(client: &ClientState) -> Seen {
    let current = client.state().clone();
    let mut scenery = current.clone();
    let shown: BTreeSet<Position> = current
        .observation
        .visible_cells
        .iter()
        .map(|c| c.position)
        .collect();
    let columns: BTreeSet<(i32, i32)> = shown.iter().map(|p| (p.x, p.y)).collect();
    let mut doors = BTreeMap::new();
    for cell in client.map_memory() {
        let (x, y) = (cell.position.x, cell.position.y);
        let near = x.unsigned_abs() <= 2 && y.unsigned_abs() <= 2;
        if shown.contains(&cell.position) || !(near || columns.contains(&(x, y))) {
            continue;
        }
        if let Some(door) = &cell.door {
            doors.insert(cell.position, door.clone());
        }
        scenery.observation.visible_cells.push(CellView {
            // Stale door appearance is stored separately; no DoorView or target
            // can be recovered from it. The geometry alone has no action handle.
            door: None,
            material: cell.material.clone(),
            key: cell.key.clone(),
            stairs_up: cell.stairs_up,
            stairs_down: cell.stairs_down,
            position: cell.position,
            wall: cell.wall,
            place_hint: cell.place_hint,
            asset: cell.asset.clone(),
        });
    }
    Seen {
        current,
        scenery,
        doors,
    }
}
