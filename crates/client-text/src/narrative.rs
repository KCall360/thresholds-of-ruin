//! The place the character is in, as a name and a sentence.
//!
//! Everything here comes from the disclosed cells: the place's extent and
//! shape from [`crate::engine::place`], its surfaces from the seen solid
//! cells. Nothing is invented: no smells, no draughts, no authored region names
//! the protocol doesn't disclose.

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

fn noun(place: &Place) -> &'static str {
    let span = |values: Vec<i32>| {
        values.iter().max().unwrap_or(&0) - values.iter().min().unwrap_or(&0) + 1
    };
    let narrow = || {
        let xs = place.columns.iter().map(|c| c.0).collect();
        let ys = place.columns.iter().map(|c| c.1).collect();
        span(xs).min(span(ys)) <= 1
    };
    match place.form {
        Form::Open => "an open space",
        Form::Passage if narrow() => "a narrow passage",
        Form::Passage => "a passage",
        Form::Alcove => "an alcove",
        Form::Chamber if place.columns.len() <= 15 => "a small chamber",
        Form::Chamber => "a chamber",
        Form::Hall => "a large hall",
    }
}

/// The place in one or two sentences: what kind of place, its floor and
/// walls, how high its ceiling is when that's notable, and where it goes on
/// out of sight.
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
    let mut text = format!("You are in {}", noun(&place));
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
    text.push('.');
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
