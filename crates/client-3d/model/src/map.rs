//! Exact disclosed map destinations and revision-bound, explicitly confirmed journeys.
use crate::{Cmd, View};
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::{Position, ReplyContext};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapDestination {
    pub key: String,
    pub position: Position,
    pub label: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapSelection {
    pub shown: ReplyContext,
    pub destination: MapDestination,
}
pub fn map_destinations(view: &View) -> Vec<MapDestination> {
    let mut columns = BTreeMap::new();
    for cell in view.terrain().filter(|cell| cell.position.z == view.feet.z) {
        let entry = columns.entry(cell.position).or_insert((cell.seen, 0usize));
        if cell.seen && !entry.0 {
            *entry = (cell.seen, 1);
        } else if cell.seen == entry.0 {
            entry.1 += 1;
        }
    }
    let current: Vec<_> = view
        .terrain()
        .filter(|cell| {
            columns
                .get(&cell.position)
                .is_some_and(|(seen, _)| *seen == cell.seen)
        })
        .collect();
    let mut keys = BTreeMap::new();
    for cell in &current {
        *keys.entry(cell.key).or_insert(0usize) += 1;
    }
    let occupied: BTreeSet<_> = view
        .actors
        .iter()
        .filter(|a| !a.is_self)
        .map(|a| a.position)
        .collect();
    let mut rows: Vec<_> = current
        .into_iter()
        .filter(|cell| {
            columns[&cell.position].1 == 1
                && keys[cell.key] == 1
                && cell.position != view.feet
                && !occupied.contains(&cell.position)
                && (!cell.wall || cell.stairs_up || cell.stairs_down)
                && cell.door != Some(false)
                && cell.trap.is_none()
        })
        .map(|cell| MapDestination {
            key: cell.key.to_owned(),
            position: cell.position,
            label: if cell.stairs_up {
                "Stairs up"
            } else if cell.stairs_down {
                "Stairs down"
            } else if cell.door.is_some() {
                "Open doorway"
            } else {
                "Known floor"
            },
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.position.y, a.position.x, a.position.z, &a.key).cmp(&(
            b.position.y,
            b.position.x,
            b.position.z,
            &b.key,
        ))
    });
    rows
}
impl MapSelection {
    pub fn select(view: &View, key: &str) -> Option<Self> {
        map_destinations(view)
            .into_iter()
            .find(|cell| cell.key == key)
            .map(|destination| Self {
                shown: view.shown.clone(),
                destination,
            })
    }
    pub fn command(&self, view: &View, ready: bool) -> Option<Cmd> {
        (ready
            && view.identify.is_none()
            && view.has_control
            && view.can_act
            && self.shown == view.shown
            && map_destinations(view)
                .iter()
                .any(|current| current == &self.destination))
        .then(|| Cmd::Travel(self.destination.key.clone()))
    }
}
