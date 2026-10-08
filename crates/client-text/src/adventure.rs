//! Scene descriptions and ways onward, using only the disclosed observer scene.
use std::collections::{BTreeMap, BTreeSet};

use tor_client_common::{surfaces, AssetTable, Palette};
use tor_protocol::*;

use crate::{
    engine::{
        place::{Opening, Place},
        prose,
    },
    narrative::Places,
    safe,
};

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
    let found: Vec<_> = place.ways(direction).collect();
    // Several ways alike that way are told apart by where each lies from
    // here, so a question can name them: "a passage to the northeast".
    let alike = |w: &crate::engine::place::Way| {
        found
            .iter()
            .filter(|o| o.kind_name() == w.kind_name())
            .count()
            > 1
    };
    let ways: Vec<Exit> = found
        .iter()
        .map(|w| Exit {
            destination: w.destination.clone(),
            label: match w.towards.filter(|_| alike(w)) {
                Some(lies) => format!("{} to the {}", w.label(), direction_name(lies)),
                None => format!("{} {toward}", w.label()),
            },
            closed: matches!(w.kind, Opening::Door { open: false, .. }),
        })
        .collect();
    if !ways.is_empty() {
        return ways;
    }
    let open = place.form == crate::engine::place::Form::Open;
    let mut seen = BTreeSet::new();
    // Places seen elsewhere guide the way only across open ground; within
    // walls the ways out are the openings.
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
                && open
        })
        .collect();
    anchors.sort_by_key(|c| (distance(c.position), &c.key));
    if anchors.is_empty() && (open || place.continues.contains(&direction)) {
        // Where the place goes on into darkness, or across open ground, a
        // direction leads as far as can be seen that way, keeping as
        // straight as it can and, within walls, inside the place.
        let straight = |p: Position| match direction {
            Direction::North | Direction::South => p.x.unsigned_abs(),
            Direction::East | Direction::West => p.y.unsigned_abs(),
            _ => p.x.unsigned_abs().abs_diff(p.y.unsigned_abs()),
        };
        let far = state
            .observation
            .visible_cells
            .iter()
            .filter(|c| {
                !c.wall
                    && c.door.is_none()
                    && c.position.z == 0
                    && bearing(c.position) == Some(direction)
                    && (open || place.contains(c.position))
            })
            .min_by_key(|c| {
                (
                    straight(c.position),
                    std::cmp::Reverse(distance(c.position)),
                    &c.key,
                )
            });
        return far
            .map(|c| Exit {
                destination: Some(c.key.clone()),
                label: format!("open ground {toward}"),
                closed: false,
            })
            .into_iter()
            .collect();
    }
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

/// The authored hint inside a place, if it has one.
fn hint<'a>(state: &'a StateView, place: &Place) -> Option<&'a str> {
    state
        .observation
        .visible_cells
        .iter()
        .filter(|c| c.place_hint && !c.wall && place.contains(c.position))
        .map(|c| c.key.as_str())
        .min()
}

/// Where a walk in `direction` goes next, after a leg that began in `before`
/// and ended in `now`, when there's nothing yet worth stopping for: the way
/// it took, and where that leg ends. A walk goes on into darkness, and along
/// a corridor while there's one way on, following its bends. It stops on
/// entering a room, at a junction or a door, or where the way runs out.
/// Whether something came into view is the caller's to judge.
pub fn onward(
    before: &StateView,
    now: &StateView,
    direction: Direction,
) -> Option<(Direction, String)> {
    use crate::engine::place::{survey, Form};
    let (was, is) = (survey(before), survey(now));
    let here = crate::narrative::here_key(now)?;
    // Arriving somewhere new is a place to stop.
    let roomy = !matches!(is.form, Form::Passage | Form::Open);
    let new_hint = hint(now, &is).is_some_and(|h| hint(before, &was) != Some(h));
    if new_hint || (roomy && was.form != is.form) {
        return None;
    }
    // On into the dark, unless an opening lies that way.
    if is.ways(direction).next().is_none()
        && (is.form == Form::Open || is.continues.contains(&direction))
    {
        return exits_from(&is, now, direction)
            .into_iter()
            .find_map(|e| e.destination)
            .filter(|d| d != here)
            .map(|d| (direction, d));
    }
    // Along a corridor, following its bends, to just before anything that
    // asks for a choice.
    crate::engine::place::corridor_ahead(now, direction).filter(|(_, to)| to != here)
}

/// What a walk would stop to look at, if it came into view: things and
/// doors, with how to name them and where they are.
pub(crate) fn sights(state: &StateView) -> Vec<(String, String, String)> {
    let o = &state.observation;
    let mut seen = Vec::new();
    for item in &o.ground_items {
        seen.push((
            format!("item:{}", item.item.id),
            prose::counted(item.item.quantity, &safe(&item.item.name)),
            whereabouts(item.position),
        ));
    }
    for cell in &o.visible_cells {
        if let Some(door) = &cell.door {
            let name = if door.name.trim().is_empty() {
                "door".to_owned()
            } else {
                safe(&door.name)
            };
            seen.push((
                format!("door:{}", door.id),
                prose::indefinite(&name),
                whereabouts(cell.position),
            ));
        }
    }
    seen
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

struct UnifiedActor<'a> {
    id: ActorTarget,
    name: &'a str,
    asset: Option<&'a str>,
    base_position: Position,
    cells: Vec<Position>,
}

fn unified_actors<'a>(actors: &'a [ActorView]) -> Vec<UnifiedActor<'a>> {
    let mut by_id: BTreeMap<ActorTarget, Vec<&'a ActorView>> = BTreeMap::new();
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
            a.id,
        )
    });
    result
}

/// A figure's name, with its size when it fills more than one cell:
/// "towering giant", "massive beast".
fn figure_name(actor: &UnifiedActor, palette: &Palette) -> String {
    let base_name = if actor.name.is_empty() {
        palette
            .resolve(words(), actor.asset)
            .copied()
            .unwrap_or("figure")
    } else {
        actor.name
    };
    let span = |axis: fn(&Position) -> i32| {
        let values = actor.cells.iter().map(axis);
        values.clone().max().unwrap_or(0) - values.min().unwrap_or(0) + 1
    };
    let height = span(|p| p.z);
    let width = span(|p| p.x).max(span(|p| p.y));
    let towering = height >= 3 && !base_name.contains("towering");
    let massive = width >= 2 && !base_name.contains("massive");
    let name = match (towering, massive) {
        (true, true) => format!("towering, massive {base_name}"),
        (true, false) => format!("towering {base_name}"),
        (false, true) => format!("massive {base_name}"),
        (false, false) => base_name.to_owned(),
    };
    safe(&name)
}

/// An injury as a word before a figure's name; none when it's unhurt.
fn injury_word(injury: Injury) -> Option<&'static str> {
    match injury {
        Injury::Healthy => None,
        Injury::Wounded => Some("wounded"),
        Injury::BadlyWounded => Some("badly wounded"),
        Injury::NearDeath => Some("gravely wounded"),
    }
}

/// Names counted and gathered by where they are, in the order each place
/// was first named: [("to the east", [("scout", 1), ("rat", 2)], 3)].
type Gathering = (String, Vec<(String, u64)>, u64);

fn gathered(entries: Vec<(String, String, u64)>) -> Vec<Gathering> {
    let mut counted: Vec<(String, String, u64)> = Vec::new();
    for (place, name, count) in entries {
        match counted
            .iter_mut()
            .find(|(p, n, _)| *p == place && *n == name)
        {
            Some((.., c)) => *c += count,
            None => counted.push((place, name, count)),
        }
    }
    let mut groups: Vec<Gathering> = Vec::new();
    for (place, name, count) in counted {
        match groups.iter_mut().find(|(p, ..)| *p == place) {
            Some((_, names, total)) => {
                names.push((name, count));
                *total += count;
            }
            None => groups.push((place, vec![(name, count)], count)),
        }
    }
    groups
}

/// "a rat and two scouts".
fn counted_list(names: &[(String, u64)]) -> String {
    let phrases: Vec<String> = names
        .iter()
        .map(|(name, count)| prose::counted(*count, name))
        .collect();
    prose::and_list(&phrases)
}

/// "There is a stone guardian to the east, and two wounded rats to the
/// north." Figures alike in the same place are counted together, and their
/// injuries go with their names.
fn figures_sentence(state: &StateView, palette: &Palette) -> Vec<String> {
    let o = &state.observation;
    let mut sentences = Vec::new();
    let mut entries = Vec::new();
    for actor in unified_actors(&o.visible_actors) {
        if actor.id == o.self_target {
            if !actor.cells.iter().any(|p| p.x == 0 && p.y == 0) {
                sentences.push(format!(
                    "You can see yourself {}.",
                    whereabouts(actor.base_position)
                ));
            }
            continue;
        }
        let injury = o
            .combat
            .as_ref()
            .and_then(|c| c.actors.iter().find(|c| c.actor == actor.id))
            .and_then(|c| injury_word(c.injury));
        let name = figure_name(&actor, palette);
        let name = injury.map_or_else(|| name.clone(), |i| format!("{i} {name}"));
        entries.push((whereabouts(actor.base_position), name, 1));
    }
    let groups = gathered(entries);
    if let Some((_, first, _)) = groups.first() {
        // "There is a scout and two rats", "There are two rats and a scout".
        let verb = if first[0].1 == 1 { "is" } else { "are" };
        let clauses: Vec<String> = groups
            .iter()
            .map(|(place, names, _)| format!("{} {place}", counted_list(names)))
            .collect();
        sentences.insert(0, format!("There {verb} {}.", gapped(&clauses)));
    }
    sentences
}

/// Clauses joined so each stands apart even when it holds a list of its own:
/// "a token to the east", "a token to the east, and a key to the north",
/// "a, b, and c".
fn gapped(clauses: &[String]) -> String {
    match clauses {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first}, and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// "A copper token lies at your feet; two stone tablets lie to the east."
fn things_sentences(state: &StateView, place: &Place) -> Vec<String> {
    let o = &state.observation;
    let mut seen = BTreeSet::new();
    let mut items: Vec<&GroundItemView> = o
        .ground_items
        .iter()
        .filter(|i| seen.insert(i.item.id))
        .collect();
    // At your feet, then nearby, then farther off.
    items.sort_by_key(|i| {
        (
            !i.reachable,
            !place.contains(i.position),
            distance(i.position),
        )
    });
    // Things alike in the same place are counted together.
    let entries = items
        .into_iter()
        .map(|item| {
            let place = if item.reachable {
                "at your feet".to_owned()
            } else if place.contains(item.position) {
                "on the floor nearby".to_owned()
            } else {
                whereabouts(item.position)
            };
            (place, safe(&item.item.name), item.item.quantity)
        })
        .collect();
    let mut told: BTreeSet<String> = BTreeSet::new();
    let clauses: Vec<String> = gathered(entries)
        .into_iter()
        .map(|(place, names, count)| {
            // Things named before in another place are "another" or "more".
            let phrases: Vec<String> = names
                .iter()
                .map(|(name, n)| match (told.insert(name.clone()), n) {
                    (true, _) => prose::counted(*n, name),
                    (false, 1) => format!("another {name}"),
                    (false, n) => format!("{} more {}", prose::number(*n), prose::plural(name)),
                })
                .collect();
            let list = prose::and_list(&phrases);
            let level = !matches!(place.as_str(), "above you" | "below you");
            let verb = match (level, count == 1) {
                (true, true) => "lies",
                (true, false) => "lie",
                (false, true) => "is",
                (false, false) => "are",
            };
            if list.starts_with(|c: char| c.is_ascii_digit()) {
                // "At your feet lie 17 arrows", not "17 arrows lie at...".
                format!("{place} {verb} {list}")
            } else {
                format!("{list} {verb} {place}")
            }
        })
        .collect();
    clauses.chunks(2).map(|pair| pair.join("; ")).collect()
}

/// Doors in sight that aren't ways out of this place.
fn other_doors(state: &StateView, place: &Place) -> Option<String> {
    let ways: BTreeSet<DoorTarget> = place
        .ways
        .iter()
        .filter_map(|w| match w.kind {
            Opening::Door { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    let mut doors = BTreeSet::new();
    let mut entries = Vec::new();
    let mut cells: Vec<&CellView> = state.observation.visible_cells.iter().collect();
    cells.sort_by_key(|c| (distance(c.position), &c.key));
    for cell in cells {
        if let Some(door) = &cell.door {
            if !ways.contains(&door.id) && doors.insert(door.id) {
                let name = if door.name.trim().is_empty() {
                    "door".to_owned()
                } else {
                    safe(&door.name)
                };
                let state = if door.open { "open" } else { "closed" };
                entries.push((whereabouts(cell.position), format!("{state} {name}"), 1));
            }
        }
    }
    let clauses: Vec<String> = gathered(entries)
        .iter()
        .map(|(place, names, _)| format!("{} {place}", counted_list(names)))
        .collect();
    (!clauses.is_empty()).then(|| format!("You can also see {}.", gapped(&clauses)))
}

const DIRECTIONS: [Direction; 10] = [
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
];

/// The ways out by kind: "An archway leads east, a passage north, and open
/// wooden doors south and west." Directions with no opening but a place seen
/// that way follow: "You can also head east."
fn ways_sentences(state: &StateView, place: &Place) -> Vec<String> {
    // Openings of one kind, with the directions they lead and how many.
    let mut kinds: Vec<(String, Vec<Direction>)> = Vec::new();
    for way in &place.ways {
        let kind = way.kind_name();
        match kinds.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, directions)) => directions.push(way.direction),
            None => kinds.push((kind, vec![way.direction])),
        }
    }
    let clauses: Vec<(String, String, bool)> = kinds
        .iter()
        .map(|(kind, directions)| {
            let mut distinct: Vec<Direction> = directions.clone();
            distinct.dedup();
            let towards: Vec<String> = distinct
                .iter()
                .map(|d| direction_name(*d).to_owned())
                .collect();
            let stairs = kind == "stairs";
            let (subject, plural) = match directions.len() {
                _ if stairs => (kind.clone(), true),
                1 => (prose::indefinite(kind), false),
                n if distinct.len() == 1 => (prose::counted(n as u64, kind), true),
                _ => (prose::plural(kind), true),
            };
            (subject, prose::and_list(&towards), plural)
        })
        .collect();
    let mut sentences = Vec::new();
    if let Some((subject, towards, plural)) = clauses.first() {
        let verb = if *plural { "lead" } else { "leads" };
        let mut parts = vec![format!("{subject} {verb} {towards}")];
        parts.extend(clauses[1..].iter().map(|(s, t, _)| format!("{s} {t}")));
        sentences.push(prose::sentence(&gapped(&parts)));
    }
    // Directions into darkness were told with the place.
    let elsewhere: Vec<String> = DIRECTIONS
        .into_iter()
        .filter(|d| place.ways(*d).next().is_none() && !place.continues.contains(d))
        .filter(|d| !exits_from(place, state, *d).is_empty())
        .map(|d| direction_name(d).to_owned())
        .collect();
    if !elsewhere.is_empty() {
        let also = if sentences.is_empty() { "" } else { "also " };
        sentences.push(format!(
            "You can {also}head {}.",
            prose::or_list(&elsewhere)
        ));
    }
    let walls = state.observation.visible_cells.iter().any(|c| c.wall);
    if sentences.is_empty() && walls && place.continues.is_empty() {
        sentences.push("You see no way onward.".into());
    }
    sentences
}

/// [`describe_with`] without a palette: every thing keeps its disclosed
/// material or name.
pub fn describe(state: &StateView) -> String {
    describe_with(state, &Palette::default())
}

/// [`describe_in`] with no places remembered.
pub fn describe_with(state: &StateView, palette: &Palette) -> String {
    describe_in(state, palette, &Places::default())
}

/// The character's condition, as the first lines of a description: HP and
/// how the run stands. The objective too when `objective` is set.
fn status_lines(state: &StateView, objective: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(c) = &state.observation.combat {
        lines.push(format!("HP {}/{}", c.hp, c.max_hp));
        if c.dead {
            lines.push("You are dead. This run has ended.".into());
        } else if c.victory && c.terminal {
            lines.push("Victory! This run has ended.".into());
        } else if c.victory {
            lines.push("Victory! You may keep exploring.".into());
        }
        if objective {
            lines.extend(
                c.objective
                    .map(|o| tor_client_common::narration::objective(o).to_owned()),
            );
        }
    }
    if state.wizard_game {
        lines.push("*** WIZARD GAME — permanently marked ***".into());
    }
    lines
}

/// The scene in prose, as the game opens: condition, objective and the
/// place. Floors, walls and unnamed figures are described by their asset
/// words where the palette holds their assets.
pub fn describe_in(state: &StateView, palette: &Palette, places: &Places) -> String {
    let mut lines = status_lines(state, true);
    lines.push(describe_place_with(state, palette, places));
    lines.join("\n")
}

/// The scene as `look` shows it: like [`describe_with`], without repeating
/// the objective, which `objective` recalls.
pub fn look_with(state: &StateView, palette: &Palette, places: &Places) -> String {
    let mut lines = status_lines(state, false);
    lines.push(describe_place_with(state, palette, places));
    lines.join("\n")
}

/// A place already described, on arriving again: its name (or what kind of
/// place it is, when it has none), its ways out when `ways` is set, and who
/// and what is in it.
pub fn brief_place_with(
    state: &StateView,
    palette: &Palette,
    places: &Places,
    ways: bool,
) -> String {
    let place = crate::engine::place::survey(state);
    let mut lines = Vec::new();
    let mut about = Vec::new();
    match crate::narrative::place_title(state) {
        Some(title) => lines.push(title),
        None => about.push(format!(
            "You are back in {}.",
            crate::narrative::place_noun(state, palette, &place, places, true)
        )),
    }
    if ways {
        about.extend(ways_sentences(state, &place));
        about.extend(exit_sentence(state));
    }
    if !about.is_empty() {
        lines.push(prose::paragraph(&about));
    }
    lines.extend(contents(state, palette, &place));
    lines.join("\n")
}

/// Where the objective's exit is, when it's in sight.
fn exit_sentence(state: &StateView) -> Option<String> {
    let key = state.observation.combat.as_ref()?.exit.as_ref()?;
    let cell = state
        .observation
        .visible_cells
        .iter()
        .find(|c| &c.key == key && !c.wall)?;
    Some(if cell.position == (Position { x: 0, y: 0, z: 0 }) {
        "You are standing at the exit.".into()
    } else {
        format!("The exit is {}.", whereabouts(cell.position))
    })
}

/// Who and what is in the place, as a paragraph.
fn contents(state: &StateView, palette: &Palette, place: &Place) -> Option<String> {
    let mut sentences = figures_sentence(state, palette);
    sentences.extend(
        things_sentences(state, place)
            .into_iter()
            .map(|s| prose::sentence(&s)),
    );
    sentences.extend(other_doors(state, place));
    (!sentences.is_empty()).then(|| prose::paragraph(&sentences))
}

/// The place alone, as on arriving there: its name, a paragraph on the place
/// and its ways out, and a paragraph on who and what is in it.
pub fn describe_place_with(state: &StateView, palette: &Palette, places: &Places) -> String {
    let place = crate::engine::place::survey(state);
    let mut lines = Vec::new();
    lines.extend(crate::narrative::place_title(state));
    let mut about = vec![crate::narrative::describe_surveyed(
        state, palette, &place, places,
    )];
    about.extend(ways_sentences(state, &place));
    about.extend(exit_sentence(state));
    lines.push(prose::paragraph(&about));
    lines.extend(contents(state, palette, &place));
    lines.join("\n")
}
