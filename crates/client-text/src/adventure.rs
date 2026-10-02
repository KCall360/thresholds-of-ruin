//! Scene descriptions and ways onward, using only the disclosed observer scene.
use std::collections::{BTreeMap, BTreeSet};

use tor_client_common::{surfaces, AssetTable, Palette};
use tor_protocol::*;

use crate::safe;

pub(crate) fn distance(p: Position) -> u64 {
    u64::from(p.x.unsigned_abs()) + u64::from(p.y.unsigned_abs()) + u64::from(p.z.unsigned_abs())
}

pub fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::North => "north",
        Direction::East => "east",
        Direction::South => "south",
        Direction::West => "west",
        Direction::NorthEast => "northeast",
        Direction::SouthEast => "southeast",
        Direction::SouthWest => "southwest",
        Direction::NorthWest => "northwest",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

fn bearing(p: Position) -> Option<Direction> {
    if p.z != 0 {
        Some(if p.z > 0 {
            Direction::Up
        } else {
            Direction::Down
        })
    } else if p.x == 0 && p.y == 0 {
        None
    } else if u64::from(p.x.unsigned_abs()) * 2 >= u64::from(p.y.unsigned_abs())
        && u64::from(p.y.unsigned_abs()) * 2 >= u64::from(p.x.unsigned_abs())
    {
        Some(match (p.x > 0, p.y > 0) {
            (true, false) => Direction::NorthEast,
            (true, true) => Direction::SouthEast,
            (false, true) => Direction::SouthWest,
            (false, false) => Direction::NorthWest,
        })
    } else if p.x.unsigned_abs() > p.y.unsigned_abs() {
        Some(if p.x > 0 {
            Direction::East
        } else {
            Direction::West
        })
    } else {
        Some(if p.y > 0 {
            Direction::South
        } else {
            Direction::North
        })
    }
}

fn whereabouts(p: Position) -> String {
    bearing(p).map_or_else(
        || "at your feet".into(),
        |d| match d {
            Direction::Up => "above you".into(),
            Direction::Down => "below you".into(),
            _ => format!("to the {}", direction_name(d)),
        },
    )
}

// Use the same visible-anchor grouping for both exits and item descriptions.
fn place_at(state: &StateView, position: Position) -> Option<&str> {
    let authored = state
        .observation
        .visible_cells
        .iter()
        .filter(|c| c.place_hint && !c.wall && c.position.z == position.z)
        .min_by_key(|c| {
            (
                (i64::from(c.position.x) - i64::from(position.x)).unsigned_abs()
                    + (i64::from(c.position.y) - i64::from(position.y)).unsigned_abs(),
                &c.key,
            )
        })
        .map(|c| c.key.as_str());
    if authored.is_some() {
        return authored;
    }
    crate::narrative::current_place_key(state)
}

pub(crate) fn in_current_place(state: &StateView, position: Position) -> bool {
    let origin = Position { x: 0, y: 0, z: 0 };
    position.z == 0 && place_at(state, position) == place_at(state, origin)
}

struct Destination {
    key: String,
    label: String,
}

/// Ways onward in a direction: each journey's destination cell key and how
/// it's described.
pub(crate) fn exits(state: &StateView, direction: Direction) -> Vec<(String, String)> {
    destinations(state, direction)
        .into_iter()
        .map(|d| (d.key, d.label))
        .collect()
}

fn destinations(state: &StateView, direction: Direction) -> Vec<Destination> {
    let cells = &state.observation.visible_cells;
    let origin = cells.iter().find(|c| distance(c.position) == 0);
    let current = place_at(state, Position { x: 0, y: 0, z: 0 });
    let mut seen = BTreeSet::new();
    let mut anchors: Vec<_> = cells
        .iter()
        .filter(|c| {
            !c.wall
                && c.place_hint
                && bearing(c.position) == Some(direction)
                && origin.is_none_or(|o| o.key != c.key)
                && current.is_none_or(|key| key != c.key)
        })
        .collect();
    anchors.sort_by_key(|c| (distance(c.position), &c.key));
    let result: Vec<_> = anchors
        .into_iter()
        .filter(|c| seen.insert(c.key.clone()))
        .map(|c| {
            let item = state
                .observation
                .ground_items
                .iter()
                .find(|i| i.position == c.position);
            Destination {
                key: c.key.clone(),
                label: item.map_or_else(
                    || format!("an open place to the {}", direction_name(direction)),
                    |i| format!("the place by the {}", safe(&i.item.name)),
                ),
            }
        })
        .collect();
    if !result.is_empty() {
        return result;
    }
    // Tier 2: Disclosed doors
    let mut doors: Vec<Destination> = Vec::new();
    for cell in cells {
        if cell.position.z == 0 && bearing(cell.position) == Some(direction) {
            if let Some(door) = &cell.door {
                if seen.insert(cell.key.clone()) {
                    let status = if door.open { "open" } else { "closed" };
                    doors.push(Destination {
                        key: cell.key.clone(),
                        label: format!(
                            "{} to the {}",
                            indefinite(&format!("{status} {}", door.name)),
                            direction_name(direction)
                        ),
                    });
                }
            }
        }
    }
    if !doors.is_empty() {
        doors.sort_by_key(|d| d.label.clone());
        return doors;
    }

    // Tier 3: Perimeter wall breaks / constrictions (archways, passages)
    let wall_map: BTreeSet<(i32, i32)> = cells
        .iter()
        .filter(|c| c.wall && c.position.z == 0)
        .map(|c| (c.position.x, c.position.y))
        .collect();

    if !wall_map.is_empty() {
        let is_constriction = |x: i32, y: i32| {
            // 1-wide horizontal opening (flanked by north and south walls)
            (wall_map.contains(&(x, y - 1)) && wall_map.contains(&(x, y + 1)))
                // 1-wide vertical opening (flanked by west and east walls)
                || (wall_map.contains(&(x - 1, y)) && wall_map.contains(&(x + 1, y)))
                // 2-wide horizontal opening
                || (wall_map.contains(&(x, y - 1)) && wall_map.contains(&(x, y + 2)))
                || (wall_map.contains(&(x, y - 2)) && wall_map.contains(&(x, y + 1)))
                // 2-wide vertical opening
                || (wall_map.contains(&(x - 1, y)) && wall_map.contains(&(x + 2, y)))
                || (wall_map.contains(&(x - 2, y)) && wall_map.contains(&(x + 1, y)))
        };

        let mut constriction_cells: Vec<&CellView> = cells
            .iter()
            .filter(|c| {
                !c.wall
                    && c.position.z == 0
                    && bearing(c.position) == Some(direction)
                    && is_constriction(c.position.x, c.position.y)
            })
            .collect();

        if !constriction_cells.is_empty() {
            let mut openings: Vec<Vec<&CellView>> = Vec::new();
            while let Some(cell) = constriction_cells.pop() {
                let mut group = vec![cell];
                let mut queue = vec![cell];
                while let Some(curr) = queue.pop() {
                    let mut i = 0;
                    while i < constriction_cells.len() {
                        let other = constriction_cells[i];
                        if (curr.position.x - other.position.x).abs() <= 1
                            && (curr.position.y - other.position.y).abs() <= 1
                        {
                            constriction_cells.swap_remove(i);
                            group.push(other);
                            queue.push(other);
                        } else {
                            i += 1;
                        }
                    }
                }
                openings.push(group);
            }

            openings.sort_by_key(|g| {
                let target = g.iter().max_by_key(|c| distance(c.position)).unwrap();
                (
                    distance(target.position),
                    target.position.x,
                    target.position.y,
                )
            });

            let mut exits: Vec<Destination> = Vec::new();
            for group in openings {
                let target = group.iter().max_by_key(|c| distance(c.position)).unwrap();
                if seen.insert(target.key.clone()) {
                    let label = if group.len() <= 2 {
                        format!("an open archway to the {}", direction_name(direction))
                    } else {
                        format!("a narrow passage to the {}", direction_name(direction))
                    };
                    exits.push(Destination {
                        key: target.key.clone(),
                        label,
                    });
                }
            }
            if !exits.is_empty() {
                if exits.len() > 1 && exits[0].label == exits[1].label {
                    for (idx, exit) in exits.iter_mut().enumerate() {
                        exit.label = format!("{} ({})", exit.label, idx + 1);
                    }
                }
                return exits;
            }
        }
    }

    // Bare floor is movement within a place, not evidence of a way onward.
    // Stairs provide an explicit exception even in an unhinted space.
    if matches!(direction, Direction::Up | Direction::Down)
        && origin.is_some_and(|c| {
            if direction == Direction::Up {
                c.stairs_up
            } else {
                c.stairs_down
            }
        })
    {
        if let Some(c) = cells
            .iter()
            .filter(|c| {
                !c.wall
                    && c.position.x == 0
                    && c.position.y == 0
                    && bearing(c.position) == Some(direction)
            })
            .min_by_key(|c| distance(c.position))
        {
            return vec![Destination {
                key: c.key.clone(),
                label: format!("the stairs {}", direction_name(direction)),
            }];
        }
    }
    vec![]
}

/// Words for the assets this client knows. A lookup falls back through dotted
/// prefixes, so `terrain.floor.stone`, which has no entry, reads as
/// `terrain.floor`. A thing whose asset has no word, or isn't in the palette,
/// keeps its disclosed material or name.
pub fn words() -> &'static AssetTable<&'static str> {
    static WORDS: std::sync::OnceLock<AssetTable<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        AssetTable::new([
            ("terrain.floor", "flagstone"),
            ("terrain.floor.cave", "packed earth"),
            ("terrain.floor.marble", "polished marble"),
            ("terrain.wall", "dressed stone"),
            ("terrain.wall.cave", "rough cave rock"),
            ("terrain.wall.marble", "polished marble"),
            ("creature", "creature"),
            ("creature.rat", "rat"),
        ])
    })
}

/// A solid cell's word: its asset's, or its material.
pub(crate) fn surface<'a>(palette: &Palette, cell: &'a CellView) -> &'a str {
    palette
        .resolve(words(), cell.asset.as_deref())
        .copied()
        .unwrap_or_else(|| surfaces::material(cell))
}

/// What an open cell itself shows underfoot in raw diagnostic regions, which
/// have no solid floor: its asset's word, or its cosmetic material.
pub(crate) fn open_surface<'a>(palette: &Palette, cell: &'a CellView) -> Option<&'a str> {
    if cell.wall {
        return None;
    }
    palette
        .resolve(words(), cell.asset.as_deref())
        .copied()
        .or_else(|| (!cell.material.is_empty()).then_some(cell.material.as_str()))
}

/// The floor under an open cell: the seen solid cell below it, or, in raw
/// diagnostic regions without one, what the open cell itself shows.
pub(crate) fn floor_material<'a>(cells: &'a [CellView], cell: &'a CellView) -> Option<&'a str> {
    floor_material_with(&Palette::default(), cells, cell)
}

/// The floor under an open cell: the seen solid cell below it, or, in raw
/// diagnostic regions without one, what the open cell itself shows.
pub(crate) fn floor_material_with<'a>(
    palette: &Palette,
    cells: &'a [CellView],
    cell: &'a CellView,
) -> Option<&'a str> {
    surfaces::floor_below(cells, cell.position)
        .map(|floor| surface(palette, floor))
        .or_else(|| open_surface(palette, cell))
}

fn indefinite(name: &str) -> String {
    crate::engine::prose::indefinite(&safe(name))
}

struct UnifiedActor<'a> {
    id: ActorId,
    name: &'a str,
    asset: Option<&'a str>,
    base_position: Position,
    cells: Vec<Position>,
}

fn unified_actors<'a>(actors: &'a [ActorView]) -> Vec<UnifiedActor<'a>> {
    let mut by_id: BTreeMap<ActorId, Vec<&'a ActorView>> = BTreeMap::new();
    for actor in actors {
        by_id.entry(actor.id).or_default().push(actor);
    }
    let mut result = Vec::new();
    for (&id, list) in &by_id {
        let mut remaining: Vec<&'a ActorView> = list.clone();
        while !remaining.is_empty() {
            let first = remaining.remove(0);
            let mut component = vec![first];
            let mut queue = vec![first];
            while let Some(curr) = queue.pop() {
                let mut i = 0;
                while i < remaining.len() {
                    let other = remaining[i];
                    if (other.position.x - curr.position.x).abs() <= 1
                        && (other.position.y - curr.position.y).abs() <= 1
                        && (other.position.z - curr.position.z).abs() <= 2
                    {
                        remaining.swap_remove(i);
                        component.push(other);
                        queue.push(other);
                    } else {
                        i += 1;
                    }
                }
            }
            component.sort_by_key(|a| (a.position.z, a.position.y, a.position.x));
            let base = component[0];
            let cells = component.iter().map(|a| a.position).collect();
            result.push(UnifiedActor {
                id,
                name: &base.name,
                asset: base.asset.as_deref(),
                base_position: base.position,
                cells,
            });
        }
    }
    result.sort_by_key(|a| {
        (
            a.base_position.z,
            a.base_position.y,
            a.base_position.x,
            a.id.0,
        )
    });
    result
}

fn format_actor(actor: &UnifiedActor, observer: ActorId, palette: &Palette) -> Option<String> {
    if actor.id == observer {
        if actor.cells.iter().any(|p| p.x == 0 && p.y == 0) {
            return None;
        } else {
            return Some(format!(
                "You see yourself {}.",
                whereabouts(actor.base_position)
            ));
        }
    }
    let base_name = if actor.name.is_empty() {
        palette
            .resolve(words(), actor.asset)
            .copied()
            .unwrap_or("figure")
    } else {
        actor.name
    };

    let min_z = actor
        .cells
        .iter()
        .map(|p| p.z)
        .min()
        .unwrap_or(actor.base_position.z);
    let max_z = actor
        .cells
        .iter()
        .map(|p| p.z)
        .max()
        .unwrap_or(actor.base_position.z);
    let height = (max_z - min_z).abs() + 1;

    let min_x = actor
        .cells
        .iter()
        .map(|p| p.x)
        .min()
        .unwrap_or(actor.base_position.x);
    let max_x = actor
        .cells
        .iter()
        .map(|p| p.x)
        .max()
        .unwrap_or(actor.base_position.x);
    let min_y = actor
        .cells
        .iter()
        .map(|p| p.y)
        .min()
        .unwrap_or(actor.base_position.y);
    let max_y = actor
        .cells
        .iter()
        .map(|p| p.y)
        .max()
        .unwrap_or(actor.base_position.y);
    let width = (max_x - min_x).abs().max((max_y - min_y).abs()) + 1;

    let full_name = if height >= 3
        && !base_name.contains("towering")
        && width >= 2
        && !base_name.contains("massive")
    {
        format!("towering, massive {base_name}")
    } else if height >= 3 && !base_name.contains("towering") {
        format!("towering {base_name}")
    } else if width >= 2 && !base_name.contains("massive") {
        format!("massive {base_name}")
    } else {
        base_name.to_string()
    };

    Some(format!(
        "You see {} {}.",
        indefinite(&full_name),
        whereabouts(actor.base_position)
    ))
}

/// [`describe_with`] without a palette: every thing keeps its disclosed
/// material or name.
pub fn describe(state: &StateView) -> String {
    describe_with(state, &Palette::default())
}

/// The scene in prose. Floors, walls and unnamed figures are described by
/// their asset words where the palette holds their assets.
pub fn describe_with(state: &StateView, palette: &Palette) -> String {
    let o = &state.observation;
    let mut lines = Vec::new();
    if let Some(c) = &o.combat {
        lines.push(format!("HP {}/{}", c.hp, c.max_hp));
        if c.dead {
            lines.push("You are dead. This run has ended.".into());
        } else if c.victory && c.terminal {
            lines.push("Victory! This run has ended.".into());
        } else if c.victory {
            lines.push("Victory! You may keep exploring.".into());
        }
        lines.extend(
            c.objective
                .map(|o| tor_client_common::narration::objective(o).to_owned()),
        );
    }
    if state.wizard_game {
        lines.push("*** WIZARD GAME — permanently marked ***".into());
    }
    lines.push(describe_place_with(state, palette));
    lines.join(
        "
",
    )
}

/// The place alone, as on arriving there: no status lines.
pub fn describe_place_with(state: &StateView, palette: &Palette) -> String {
    let o = &state.observation;
    let mut lines = Vec::new();
    if let Some(title) = crate::narrative::place_title(state) {
        lines.push(title);
    }
    lines.push(crate::narrative::synthesize_room(state, palette));
    let mut seen = BTreeSet::new();
    // Things alike in the same place are counted together.
    let mut things: Vec<(String, String, String, u64)> = Vec::new();
    for item in &o.ground_items {
        if seen.insert(item.item.id) {
            let place = if item.reachable {
                "at your feet".into()
            } else if in_current_place(state, item.position) {
                "on the floor nearby".into()
            } else {
                whereabouts(item.position)
            };
            let name = safe(&item.item.name);
            let identity = format!("{name}|{}|{}", item.item.description, item.item.appearance);
            match things
                .iter_mut()
                .find(|(i, p, ..)| *i == identity && *p == place)
            {
                Some((.., count)) => *count += item.item.quantity,
                None => things.push((identity, place, name, item.item.quantity)),
            }
        }
    }
    for (_, place, name, count) in things {
        lines.push(format!(
            "You see {} {place}.",
            crate::engine::prose::counted(count, &name)
        ));
    }
    let mut doors = BTreeSet::new();
    for cell in &o.visible_cells {
        if let Some(door) = &cell.door {
            if doors.insert(door.id) {
                lines.push(format!(
                    "You see {} {}.",
                    indefinite(&format!(
                        "{} {}",
                        if door.open { "open" } else { "closed" },
                        door.name
                    )),
                    whereabouts(cell.position)
                ));
            }
        }
    }
    let unified = unified_actors(&o.visible_actors);
    for actor in &unified {
        if let Some(desc) = format_actor(actor, o.actor, palette) {
            lines.push(desc);
            if let Some(injury) = o
                .combat
                .as_ref()
                .and_then(|c| c.actors.iter().find(|c| c.actor == actor.id))
            {
                lines.push(format!(
                    "It looks {}.",
                    tor_client_common::narration::injury(injury.injury)
                ));
            }
        }
    }
    let ways: Vec<_> = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::NorthEast,
        Direction::SouthEast,
        Direction::SouthWest,
        Direction::NorthWest,
        Direction::Up,
        Direction::Down,
    ]
    .into_iter()
    .filter(|d| !destinations(state, *d).is_empty())
    .map(direction_name)
    .collect();
    if !ways.is_empty() {
        let ways: Vec<String> = ways.into_iter().map(String::from).collect();
        lines.push(format!(
            "You can head {}.",
            crate::engine::prose::or_list(&ways)
        ));
    }
    lines.join("\n")
}
