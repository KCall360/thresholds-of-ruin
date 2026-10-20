//! Appearance-only boundary. Acquisition uses ordinary disclosure; hydration
//! converts frozen values without consulting live world, catalog or entities.
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol as p;
use tor_simulation as s;

#[derive(Debug)]
pub struct PreparedMapProjection {
    pub cells: Vec<p::RememberedMapCell>,
    pub complete: bool,
    pub ambiguous: bool,
    pub limited: bool,
    pub work: s::MapWorkCounts,
    pub(crate) root: s::MapKnowledgeRoot,
}

pub(crate) fn inputs(
    view: &s::Observation,
    terrain: crate::adapt::Terrain<'_>,
) -> Vec<s::MapCellInput> {
    let mut items: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for item in &view.ground_items {
        items
            .entry(item.location)
            .or_default()
            .push(s::MapItemAppearance {
                class: item.class,
                description: item.description.clone(),
                name: item.name.clone(),
                appearance: item.appearance.clone(),
                identified: item.identified,
                quantity: item.quantity,
                asset: item.asset.clone(),
            });
    }
    let up: BTreeSet<_> = view
        .exits
        .iter()
        .filter(|e| e.direction == tor_world::Direction::Up)
        .map(|e| e.location)
        .collect();
    let down: BTreeSet<_> = view
        .exits
        .iter()
        .filter(|e| e.direction == tor_world::Direction::Down)
        .map(|e| e.location)
        .collect();
    let visible: BTreeMap<_, _> = view.visible_cells.iter().map(|c| (c.location, c)).collect();
    visible
        .into_iter()
        .map(|(location, cell)| {
            let [floor, wall, door] = terrain(location.region);
            s::MapCellInput {
                location,
                appearance: s::MapAppearance {
                    wall: cell.wall,
                    material: cell.material.into(),
                    asset: if cell.wall { wall } else { floor },
                    place_hint: cell.place_hint,
                    stairs_up: up.contains(&location),
                    stairs_down: down.contains(&location),
                    door: cell.door.map(|d| s::MapDoorAppearance {
                        name: crate::adapt::door_appearance(cell.door_secret).0.into(),
                        description: crate::adapt::door_appearance(cell.door_secret).1.into(),
                        open: d.open,
                        asset: door,
                    }),
                    trap: cell.trap.as_ref().map(|trap| s::MapTrapAppearance {
                        family: trap.family,
                    }),
                    ground_items: items.remove(&location).unwrap_or_default(),
                },
            }
        })
        .collect()
}

pub(crate) fn wire_cell(
    cell: s::ProjectedMapCell,
    salt: &str,
    actor: p::ActorId,
) -> p::RememberedMapCell {
    let a = &cell.appearance;
    p::RememberedMapCell {
        key: crate::adapt::cell_key(salt, actor.0, cell.location),
        position: p::Position {
            x: cell.position.x,
            y: cell.position.y,
            z: cell.position.z,
        },
        wall: a.wall,
        material: a.material.clone(),
        asset: a.asset.clone(),
        place_hint: a.place_hint,
        stairs_up: a.stairs_up,
        stairs_down: a.stairs_down,
        door: a.door.as_ref().map(|d| p::MapDoorAppearance {
            name: d.name.clone(),
            description: d.description.clone(),
            open: d.open,
            asset: d.asset.clone(),
        }),
        trap: a.trap.as_ref().map(|trap| p::MapTrapAppearance {
            family: trap.family.map(crate::adapt::trap_family),
        }),
        ground_items: a
            .ground_items
            .iter()
            .map(|i| p::MapItemAppearance {
                class: crate::adapt::item_class(i.class),
                description: i.description.clone(),
                name: i.name.clone(),
                appearance: i.appearance.clone(),
                identified: i.identified,
                quantity: i.quantity,
                asset: i.asset.clone(),
            })
            .collect(),
    }
}
