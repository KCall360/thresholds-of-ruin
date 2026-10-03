//! The place the character is in, as a name and a description.
//!
//! The facts come from the disclosed cells: the place's extent and shape from
//! [`crate::engine::place`], its surfaces from the seen solid cells. Atmosphere
//! (mood words, smells, sounds) colours them and is fixed per place, but has
//! no effect on play and never claims something the game hasn't disclosed.

use tor_client_common::{surfaces, Palette};
use tor_protocol::{Position, StateView};

use crate::{
    adventure::{floor_material_with, surface},
    engine::{
        place::{self, Form, Place},
        prose,
        scene::{direction_name, distance},
    },
    safe,
};

/// The cell that stands for the place the character is in, for naming it: an
/// authored place hint inside the place, or else the open cell nearest its
/// middle.
pub fn current_place_anchor(state: &StateView) -> Option<(&str, Position)> {
    let place = place::survey(state);
    let cells = &state.observation.visible_cells;
    let inside = |c: &&tor_protocol::CellView| {
        !c.wall && c.position.z == 0 && place.columns.contains(&(c.position.x, c.position.y))
    };
    let authored = cells
        .iter()
        .filter(|c| c.place_hint)
        .filter(inside)
        .min_by_key(|c| (distance(c.position), &c.key));
    if let Some(c) = authored {
        return Some((c.key.as_str(), c.position));
    }
    let count = place.columns.len() as i64;
    let (sx, sy) = place.columns.iter().fold((0i64, 0i64), |(sx, sy), (x, y)| {
        (sx + i64::from(*x), sy + i64::from(*y))
    });
    let (cx, cy) = (sx / count.max(1), sy / count.max(1));
    cells
        .iter()
        .filter(inside)
        .min_by_key(|c| {
            (
                (i64::from(c.position.x) - cx).abs() + (i64::from(c.position.y) - cy).abs(),
                &c.key,
            )
        })
        .map(|c| (c.key.as_str(), c.position))
}

/// The anchor key of the character's current place.
pub fn current_place_key(state: &StateView) -> Option<&str> {
    current_place_anchor(state).map(|(k, _)| k)
}

/// The place's name, when the character has one for it.
pub fn place_title(state: &StateView) -> Option<String> {
    let key = current_place_key(state)?;
    state
        .observation
        .places
        .iter()
        .find(|p| p.key == key && !p.name.is_empty())
        .map(|p| safe(&p.name))
}

/// A stable number for a place, so its atmosphere is the same every time it's
/// described, in every client and after every reload of the same save.
fn key_hash(key: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn seed(state: &StateView) -> usize {
    current_place_key(state).map_or(0, key_hash) as usize
}

/// What the place is mostly made of, for its atmosphere.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fabric {
    Stone,
    Timber,
    Earth,
    Unknown,
}

fn fabric(state: &StateView, palette: &Palette) -> Fabric {
    let cells = &state.observation.visible_cells;
    let mut words: Vec<String> = surfaces::roles_by(cells, |cell| surface(palette, cell))
        .walls
        .into_iter()
        .map(str::to_lowercase)
        .collect();
    if let Some(here) = cells
        .iter()
        .find(|c| c.position == Position { x: 0, y: 0, z: 0 } && !c.wall)
    {
        words.extend(floor_material_with(palette, cells, here).map(str::to_lowercase));
    }
    let any = |needles: &[&str]| words.iter().any(|w| needles.iter().any(|n| w.contains(n)));
    if any(&["stone", "marble", "rock", "flagstone"]) {
        Fabric::Stone
    } else if any(&["wood", "timber"]) {
        Fabric::Timber
    } else if any(&["earth", "dirt", "loam"]) {
        Fabric::Earth
    } else {
        Fabric::Unknown
    }
}

/// Mood words for places. They colour the description and change nothing
/// in the game.
const EPITHETS: &[&str] = &[
    "quiet", "dim", "shadowed", "cold", "drafty", "dusty", "still", "echoing",
];

/// The place's kind, with a mood word: "a small, dusty chamber".
fn noun(place: &Place, epithet: &str) -> String {
    let span = |values: Vec<i32>| {
        values.iter().max().unwrap_or(&0) - values.iter().min().unwrap_or(&0) + 1
    };
    let narrow = || {
        let xs = place.columns.iter().map(|c| c.0).collect();
        let ys = place.columns.iter().map(|c| c.1).collect();
        span(xs).min(span(ys)) <= 1
    };
    let (size, kind) = match place.form {
        Form::Open => ("open", "space"),
        Form::Passage if narrow() => ("narrow", "passage"),
        Form::Passage => ("", "passage"),
        Form::Alcove => ("", "alcove"),
        Form::Chamber if place.columns.len() <= 15 => ("small", "chamber"),
        Form::Chamber => ("", "chamber"),
        Form::Hall => ("large", "hall"),
    };
    let phrase = if size.is_empty() {
        format!("{epithet} {kind}")
    } else {
        format!("{size}, {epithet} {kind}")
    };
    prose::indefinite(&phrase)
}

/// One sentence of atmosphere for the place, chosen by what it's made of and
/// fixed for the place. Flavour only: it has no effect on play.
pub fn atmosphere(state: &StateView, palette: &Palette) -> &'static str {
    const STONE: &[&str] = &[
        "The air is cool and still, carrying a faint scent of ancient dust.",
        "A quiet chill lingers among the stones, where faint echoes answer your breath.",
        "Shadows pool softly in the corners of the masonry.",
        "A dry, still quiet hangs across the quarried rock.",
    ];
    const TIMBER: &[&str] = &[
        "The dry aroma of seasoned timber lingers in the enclosed space.",
        "Old boards creak faintly as the air shifts.",
    ];
    const EARTH: &[&str] = &[
        "The scent of cool earth hangs faintly in the air.",
        "A damp, loamy hush settles over everything.",
    ];
    let choices = match fabric(state, palette) {
        Fabric::Stone => STONE,
        Fabric::Timber => TIMBER,
        Fabric::Earth => EARTH,
        Fabric::Unknown => &["The air is cool and still."],
    };
    choices[seed(state) % choices.len()]
}

/// What `smell` notices here. Flavour only.
pub fn smell(state: &StateView, palette: &Palette) -> &'static str {
    match fabric(state, palette) {
        Fabric::Stone => {
            "The air smells cool and dry, with the faint, mineral scent of quarried stone."
        }
        Fabric::Timber => "You detect the faint, dry scent of aged timber.",
        Fabric::Earth => "The rich, damp aroma of earth and loam fills the air.",
        Fabric::Unknown => "The air carries no distinct scent.",
    }
}

/// What `listen` hears here. Flavour only, and never a claim about what is
/// happening: creatures in sight can be heard, nothing more.
pub fn listen(state: &StateView) -> &'static str {
    let others = state
        .observation
        .visible_actors
        .iter()
        .any(|a| a.id != state.observation.actor);
    if others {
        "You hear the faint scuffle and breathing of creatures nearby."
    } else {
        "You listen closely. Aside from the faint whisper of air across the stone, all is quiet."
    }
}

/// The place in a few sentences: what kind of place, its floor and walls,
/// its atmosphere, how high its ceiling is when that's notable, and where it
/// goes on out of sight.
pub fn describe_place(state: &StateView, palette: &Palette) -> String {
    let place = place::survey(state);
    let cells = &state.observation.visible_cells;
    let here = cells
        .iter()
        .find(|c| c.position == Position { x: 0, y: 0, z: 0 } && !c.wall);
    let floor = here.and_then(|c| floor_material_with(palette, cells, c));
    let walls: Vec<String> = surfaces::roles_by(cells, |cell| surface(palette, cell))
        .walls
        .into_iter()
        .map(safe)
        .collect();
    let epithet = EPITHETS[seed(state) % EPITHETS.len()];
    let mut text = format!("You are in {}", noun(&place, epithet));
    match (floor, walls.is_empty()) {
        (Some(floor), false) => text.push_str(&format!(
            " with {} floor and walls of {}",
            prose::indefinite(&safe(floor)),
            prose::and_list(&walls)
        )),
        (Some(floor), true) => {
            text.push_str(&format!(" with {} floor", prose::indefinite(&safe(floor))))
        }
        (None, false) => text.push_str(&format!(" with walls of {}", prose::and_list(&walls))),
        (None, true) => {}
    }
    text.push_str(". ");
    text.push_str(atmosphere(state, palette));
    if let Some((_, height)) = surfaces::ceiling_above(cells, Position { x: 0, y: 0, z: 0 }) {
        if height >= 4 {
            text.push_str(" The ceiling is high above you.");
        }
    }
    if !place.continues.is_empty() {
        let ways: Vec<String> = place
            .continues
            .iter()
            .map(|d| direction_name(*d).to_owned())
            .collect();
        text.push_str(&format!(
            " It goes on out of sight to the {}.",
            prose::and_list(&ways)
        ));
    }
    text
}
