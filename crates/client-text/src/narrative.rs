//! The place the character is in, as a name and a description.
//!
//! The facts come from the disclosed cells: the place's extent and shape from
//! [`crate::engine::place`], its surfaces from the seen solid cells. Atmosphere
//! ([`crate::engine::atmosphere`]) colours them and is fixed per place, but
//! has no effect on play and never claims something the game hasn't
//! disclosed.

use std::collections::BTreeMap;

use tor_client_common::{surfaces, Palette};
use tor_protocol::{Position, StateView};

use crate::{
    adventure::{floor_material_with, surface},
    engine::{
        atmosphere::{self, Atmosphere, Fabric},
        place::{self, Form, Place},
        prose,
        scene::{direction_name, distance},
    },
    safe,
};

/// The cell that stands for the place the character is in: an authored place
/// hint inside the place, or else one of its open cells. Either way it's the
/// one with the lowest key, so it doesn't depend on where in the place the
/// character stands, and keys are fixed for the game, so neither does it
/// change between visits.
pub fn current_place_anchor(state: &StateView) -> Option<(&str, Position)> {
    current_place_anchor_in(state, &place::survey(state))
}

fn current_place_anchor_in<'a>(state: &'a StateView, place: &Place) -> Option<(&'a str, Position)> {
    let inside = |c: &&tor_protocol::CellView| {
        !c.wall && c.position.z == 0 && place.columns.contains(&(c.position.x, c.position.y))
    };
    let cells = &state.observation.visible_cells;
    cells
        .iter()
        .filter(inside)
        .filter(|c| c.place_hint)
        .min_by_key(|c| (&c.key, distance(c.position)))
        .or_else(|| {
            cells
                .iter()
                .filter(inside)
                .min_by_key(|c| (&c.key, distance(c.position)))
        })
        .map(|c| (c.key.as_str(), c.position))
}

/// Which place each cell was first seen as part of, so a place without an
/// authored hint keeps the key it was first given while more of it comes
/// into sight. A hinted place is always known by its hint. This isn't game
/// state: a client that has forgotten it starts again from what it sees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Places {
    by_cell: BTreeMap<String, String>,
    /// The cell the character stood on when this client began, which `go to
    /// start` returns to.
    pub start: Option<String>,
}

/// The key of open ground that has no authored hint.
const OPEN_GROUND: &str = "open ground";

/// No places remembered.
pub static NO_PLACES: Places = Places {
    by_cell: BTreeMap::new(),
    start: None,
};

/// The key of the cell the character stands on.
pub fn here_key(state: &StateView) -> Option<&str> {
    state
        .observation
        .visible_cells
        .iter()
        .find(|c| c.position == Position { x: 0, y: 0, z: 0 } && !c.wall)
        .map(|c| c.key.as_str())
}

impl Places {
    /// Remember where the character begins, once.
    pub fn begin(&mut self, state: &StateView) {
        if self.start.is_none() {
            self.start = here_key(state).map(str::to_owned);
        }
    }

    /// The key of the place the character is in.
    pub fn key(&self, state: &StateView) -> Option<String> {
        self.key_in(state, &place::survey(state))
    }

    /// [`Places::key`] for a place already surveyed: its hint, else the key
    /// most of its cells were first seen under, else its lowest cell key.
    pub fn key_in(&self, state: &StateView, place: &Place) -> Option<String> {
        let cells: Vec<&tor_protocol::CellView> = state
            .observation
            .visible_cells
            .iter()
            .filter(|c| {
                !c.wall
                    && c.position.z == 0
                    && place.columns.contains(&(c.position.x, c.position.y))
            })
            .collect();
        if let Some(hint) = cells.iter().filter(|c| c.place_hint).min_by_key(|c| &c.key) {
            return Some(hint.key.clone());
        }
        // Open ground without walls or hints has no bounds to tell one part
        // from another: it's all one place, with one atmosphere.
        if place.form == Form::Open {
            return Some(OPEN_GROUND.to_owned());
        }
        let mut votes: BTreeMap<&str, usize> = BTreeMap::new();
        for cell in &cells {
            if let Some(key) = self.by_cell.get(&cell.key) {
                *votes.entry(key).or_default() += 1;
            }
        }
        votes
            .into_iter()
            .max_by_key(|(key, n)| (*n, std::cmp::Reverse(*key)))
            .map(|(key, _)| key.to_owned())
            .or_else(|| cells.iter().map(|c| &c.key).min().cloned())
    }

    /// Remember the place the character is in under its key, and return
    /// the key.
    pub fn learn(&mut self, state: &StateView) -> Option<String> {
        let place = place::survey(state);
        let key = self.key_in(state, &place)?;
        for cell in &state.observation.visible_cells {
            if !cell.wall
                && cell.position.z == 0
                && place.columns.contains(&(cell.position.x, cell.position.y))
            {
                self.by_cell
                    .entry(cell.key.clone())
                    .or_insert_with(|| key.clone());
            }
        }
        Some(key)
    }
}

/// The anchor key of the character's current place.
pub fn current_place_key(state: &StateView) -> Option<&str> {
    current_place_anchor(state).map(|(k, _)| k)
}

/// The key of the place the character can name: an authored place hint
/// inside it. Only hinted places are learned, so only they can be renamed.
pub fn named_place_key(state: &StateView) -> Option<&str> {
    let place = place::survey(state);
    let (key, _) = current_place_anchor_in(state, &place)?;
    state
        .observation
        .visible_cells
        .iter()
        .any(|c| c.key == key && c.place_hint)
        .then_some(key)
}

/// The place's name, when the character has one for it.
pub fn place_title(state: &StateView) -> Option<String> {
    let key = current_place_key(state)?;
    state
        .observation
        .places
        .iter()
        .find(|p| {
            p.key == key
                && !p.name.is_empty()
                && p.origin != tor_protocol::PlaceNameOrigin::Invented
        })
        .map(|p| safe(&p.name))
}

/// The floor under the character and the walls in sight, as words.
fn surfaces_here(state: &StateView, palette: &Palette) -> (Option<String>, Vec<String>) {
    let cells = &state.observation.visible_cells;
    let here = cells
        .iter()
        .find(|c| c.position == Position { x: 0, y: 0, z: 0 } && !c.wall);
    let floor = here
        .and_then(|c| floor_material_with(palette, cells, c))
        .map(safe);
    let walls = surfaces::roles_by(cells, |cell| surface(palette, cell))
        .walls
        .into_iter()
        .map(safe)
        .collect();
    (floor, walls)
}

/// One column across, however long.
fn narrow(place: &Place) -> bool {
    let span = |values: Vec<i32>| {
        values.iter().max().unwrap_or(&0) - values.iter().min().unwrap_or(&0) + 1
    };
    let xs = place.columns.iter().map(|c| c.0).collect();
    let ys = place.columns.iter().map(|c| c.1).collect();
    span(xs).min(span(ys)) <= 1
}

fn atmosphere_of(
    state: &StateView,
    palette: &Palette,
    place: &Place,
    places: &Places,
) -> Atmosphere {
    let (floor, walls) = surfaces_here(state, palette);
    let fabric = Fabric::of(walls.iter().map(String::as_str), floor.as_deref());
    let key = places.key_in(state, place).unwrap_or_default();
    atmosphere::of(&key, fabric, place.form, narrow(place))
}

/// The atmosphere of the place the character is in.
pub fn atmosphere(state: &StateView, palette: &Palette, places: &Places) -> Atmosphere {
    atmosphere_of(state, palette, &place::survey(state), places)
}

/// What `smell` notices here. Flavour only.
pub fn smell(state: &StateView, palette: &Palette, places: &Places) -> String {
    atmosphere(state, palette, places).smell.to_owned()
}

/// What `listen` hears here. Flavour only, and never a claim about what is
/// happening: the place's own sound, and a reminder of whoever is in sight.
pub fn listen(state: &StateView, palette: &Palette, places: &Places) -> String {
    let mut text = atmosphere(state, palette, places).sound.to_owned();
    // Each figure once, however many cells it fills; alike ones counted.
    let mut seen = std::collections::BTreeSet::new();
    let mut names: Vec<(String, u64)> = Vec::new();
    for a in &state.observation.visible_actors {
        if a.id == state.observation.actor || !seen.insert(a.id) {
            continue;
        }
        let name = safe(if a.name.is_empty() { "figure" } else { &a.name });
        match names.iter_mut().find(|(n, _)| *n == name) {
            Some((_, count)) => *count += 1,
            None => names.push((name, 1)),
        }
    }
    let others: Vec<String> = names
        .iter()
        .map(|(name, count)| prose::counted_definite(*count, name))
        .collect();
    if !others.is_empty() {
        text.push_str(&format!(
            " You keep an ear on {}.",
            prose::and_list(&others)
        ));
    }
    text
}

/// The place's kind with its mood word, "the dusty chamber", or with an
/// indefinite article when `definite` isn't set.
pub fn place_noun(
    state: &StateView,
    palette: &Palette,
    place: &Place,
    places: &Places,
    definite: bool,
) -> String {
    let mood = atmosphere_of(state, palette, place, places).mood;
    let phrase = noun(place, mood);
    match phrase.split_once(' ') {
        Some((_, rest)) if definite => format!("the {rest}"),
        _ => phrase,
    }
}

/// The place's kind, with a mood word: "a small, dusty chamber".
fn noun(place: &Place, mood: &str) -> String {
    let (size, kind) = match place.form {
        Form::Open => ("open", "space"),
        Form::Passage if narrow(place) => ("narrow", "passage"),
        Form::Passage => ("", "passage"),
        Form::Alcove => ("", "alcove"),
        Form::Chamber if place.columns.len() <= 15 => ("small", "chamber"),
        Form::Chamber => ("", "chamber"),
        Form::Hall => ("large", "hall"),
    };
    let phrase = if size.is_empty() {
        format!("{mood} {kind}")
    } else {
        format!("{size}, {mood} {kind}")
    };
    prose::indefinite(&phrase)
}

/// What the place is made of, after its kind: " of stone", " with a
/// flagstone floor and walls of dressed stone".
fn fabric_phrase(floor: Option<&str>, walls: &[String]) -> String {
    match (floor, walls) {
        (Some(floor), [wall]) if floor == wall => format!(" of {floor}"),
        (Some(floor), []) => format!(" with {} floor", prose::indefinite(floor)),
        (Some(floor), walls) => format!(
            " with {} floor and walls of {}",
            prose::indefinite(floor),
            prose::and_list(walls)
        ),
        (None, []) => String::new(),
        (None, walls) => format!(" with walls of {}", prose::and_list(walls)),
    }
}

/// The place in a few sentences: what kind of place, what it's made of, its
/// atmosphere, how high its ceiling is when that's notable, and where it goes
/// on out of sight.
pub fn describe_place(state: &StateView, palette: &Palette, places: &Places) -> String {
    describe_surveyed(state, palette, &place::survey(state), places)
}

/// [`describe_place`] for a place already surveyed.
pub fn describe_surveyed(
    state: &StateView,
    palette: &Palette,
    place: &Place,
    places: &Places,
) -> String {
    let (floor, walls) = surfaces_here(state, palette);
    let mood = atmosphere_of(state, palette, place, places);
    let mut sentences = vec![format!(
        "You are in {}{}.",
        noun(place, mood.mood),
        fabric_phrase(floor.as_deref(), &walls)
    )];
    sentences.extend(mood.description.iter().map(|s| (*s).to_owned()));
    let cells = &state.observation.visible_cells;
    if let Some((_, height)) = surfaces::ceiling_above(cells, Position { x: 0, y: 0, z: 0 }) {
        if height >= 4 {
            sentences.push("The ceiling is high above you.".into());
        }
    }
    // What isn't seen fades into darkness, and each such way can be walked.
    match place.continues.as_slice() {
        [] => {}
        all if all.len() == 8 && place.form == Form::Open => {
            sentences.push("Open ground stretches away into darkness on every side.".into())
        }
        all if all.len() == 8 => sentences.push("It fades into darkness on every side.".into()),
        ways => {
            let ways: Vec<String> = ways.iter().map(|d| direction_name(*d).to_owned()).collect();
            sentences.push(format!(
                "To the {} it fades into darkness.",
                prose::and_list(&ways)
            ));
        }
    }
    prose::paragraph(&sentences)
}
