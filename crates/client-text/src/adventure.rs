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

/// A way onward in one direction.
pub(crate) struct Exit {
    /// Where a journey that way ends.
    pub destination: Option<String>,
    /// "an archway to the east".
    pub label: String,
    /// A closed door: the way is there, but shut.
    pub closed: bool,
}

/// Ways onward in a direction: the place's openings, or else an authored
/// anchor in another place seen that way.
pub(crate) fn exits(state: &StateView, direction: Direction) -> Vec<Exit> {
    exits_from(&crate::engine::place::survey(state), state, direction)
}

/// [`exits`] for a place already surveyed.
fn exits_from(
    place: &crate::engine::place::Place,
    state: &StateView,
    direction: Direction,
) -> Vec<Exit> {
    use crate::engine::place::Opening;
    let toward = match direction {
        Direction::Up | Direction::Down => direction_name(direction).to_owned(),
        _ => format!("to the {}", direction_name(direction)),
    };
    let ways: Vec<Exit> = place
        .ways(direction)
        .map(|w| Exit {
            destination: w.destination.clone(),
            label: format!("{} {toward}", w.label()),
            closed: matches!(w.kind, Opening::Door { open: false, .. }),
        })
        .collect();
    if !ways.is_empty() {
        return ways;
    }
    let mut seen = BTreeSet::new();
    let mut anchors: Vec<_> = state
        .observation
        .visible_cells
        .iter()
        .filter(|c| {
            !c.wall
                && c.place_hint
                && c.position.z == 0
                && bearing(c.position) == Some(direction)
                && !place.contains(c.position)
        })
        .collect();
    anchors.sort_by_key(|c| (distance(c.position), &c.key));
    anchors
        .into_iter()
        .filter(|c| seen.insert(c.key.clone()))
        .map(|c| {
            let item = state
                .observation
                .ground_items
                .iter()
                .find(|i| i.position == c.position);
            Exit {
                destination: Some(c.key.clone()),
                label: item.map_or_else(
                    || format!("an open place {toward}"),
                    |i| format!("the place by the {}", safe(&i.item.name)),
                ),
                closed: false,
            }
        })
        .collect()
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
    lines.push(crate::narrative::describe_place(state, palette));
    let place = crate::engine::place::survey(state);
    let mut seen = BTreeSet::new();
    // Things alike in the same place are counted together.
    let mut things: Vec<(String, String, String, u64)> = Vec::new();
    for item in &o.ground_items {
        if seen.insert(item.item.id) {
            let place = if item.reachable {
                "at your feet".into()
            } else if place.contains(item.position) {
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
    .filter(|d| exits_from(&place, state, *d).iter().any(|e| !e.closed))
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
