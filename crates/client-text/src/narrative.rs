//! Rich interactive fiction narrative presentation and world modeling.
//!
//! Synthesizes sparse observation data from the server into an authentic,
//! evocative, and vibrant interactive fiction prose environment.

use tor_client_common::{surfaces, Palette};
use tor_protocol::{CellView, Position, StateView};

use crate::{
    adventure::{distance, floor_material, floor_material_with, surface},
    safe,
};

/// Deterministic hash for consistent procedural atmosphere from a stable place key.
fn key_hash(key: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Discovers or deterministically designates the anchor representing the current space.
pub fn current_place_anchor(state: &StateView) -> Option<(&str, Position)> {
    let o = &state.observation;
    // 1. Look for authored place hints at z = 0
    let authored = o
        .visible_cells
        .iter()
        .filter(|c| c.place_hint && !c.wall && c.position.z == 0)
        .min_by_key(|c| {
            (
                c.position.x.unsigned_abs() as u64 + c.position.y.unsigned_abs() as u64,
                &c.key,
            )
        });
    if let Some(c) = authored {
        return Some((c.key.as_str(), c.position));
    }

    // 2. Client-spawned place hint: compute centroid of walkable component connected to (0, 0, 0)
    let mut walkable: std::collections::BTreeMap<(i32, i32), &CellView> =
        std::collections::BTreeMap::new();
    for cell in &o.visible_cells {
        if !cell.wall && cell.position.z == 0 {
            walkable.insert((cell.position.x, cell.position.y), cell);
        }
    }
    if walkable.is_empty() {
        return None;
    }
    let mut component: Vec<&CellView> = Vec::new();
    let mut visited: std::collections::BTreeSet<(i32, i32)> = std::collections::BTreeSet::new();
    let mut queue = vec![(0, 0)];
    visited.insert((0, 0));
    while let Some((x, y)) = queue.pop() {
        if let Some(&cell) = walkable.get(&(x, y)) {
            component.push(cell);
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let next = (x + dx, y + dy);
                if !visited.contains(&next) && walkable.contains_key(&next) {
                    visited.insert(next);
                    queue.push(next);
                }
            }
        }
    }
    if component.is_empty() {
        return None;
    }
    let count = component.len() as i64;
    let sum_x: i64 = component.iter().map(|c| c.position.x as i64).sum();
    let sum_y: i64 = component.iter().map(|c| c.position.y as i64).sum();
    let cx = sum_x / count;
    let cy = sum_y / count;

    let closest = component.into_iter().min_by_key(|c| {
        (
            (c.position.x as i64 - cx).abs() + (c.position.y as i64 - cy).abs(),
            &c.key,
        )
    })?;
    Some((closest.key.as_str(), closest.position))
}

/// Discovers the anchor key representing the character's current place.
pub fn current_place_key(state: &StateView) -> Option<&str> {
    current_place_anchor(state).map(|(k, _)| k)
}

/// Returns an evocative room title for the current place.
///
/// Priority:
/// 1. Player-named / authored place from `state.observation.places`.
/// 2. None (suppresses arbitrary procedural headers like "Quiet Stone Chamber").
pub fn place_title(state: &StateView) -> Option<String> {
    let key = current_place_key(state)?;
    if let Some(named) = state.observation.places.iter().find(|p| p.key == key) {
        if !named.name.is_empty() {
            return Some(safe(&named.name));
        }
    }
    None
}

/// Synthesizes the environment into an integrated prose paragraph.
pub fn synthesize_room(state: &StateView, palette: &Palette) -> String {
    let o = &state.observation;
    let key = current_place_key(state).unwrap_or("");
    let seed = key_hash(key);

    let floor_cell = o
        .visible_cells
        .iter()
        .find(|c| distance(c.position) == 0 && !c.wall);
    if floor_cell.is_none() {
        return "Your surroundings".into();
    }
    let floor_mat = floor_cell.and_then(|c| floor_material_with(palette, &o.visible_cells, c));

    let wall_materials = surfaces::roles_by(&o.visible_cells, |cell| surface(palette, cell)).walls;
    let walls_str = wall_materials
        .iter()
        .map(|w| safe(w))
        .collect::<Vec<_>>()
        .join(" and ");
    let walls_clause = if wall_materials.is_empty() {
        String::new()
    } else {
        format!(" You can see walls of {walls_str}.")
    };

    let material_prefix =
        if floor_mat.is_some_and(|m| m.contains("stone")) || walls_str.contains("stone") {
            "Stone"
        } else if floor_mat.is_some_and(|m| m.contains("wood")) || walls_str.contains("wood") {
            "Timber"
        } else if floor_mat.is_some_and(|m| m.contains("dirt") || m.contains("earth"))
            || walls_str.contains("dirt")
            || walls_str.contains("earth")
        {
            "Earthen"
        } else if floor_mat.is_some_and(|m| m.contains("marble")) || walls_str.contains("marble") {
            "Marble"
        } else {
            "Ancient"
        };

    let walkable: Vec<_> = o
        .visible_cells
        .iter()
        .filter(|c| !c.wall && c.position.z == 0)
        .collect();
    let has_stairs = o.visible_cells.iter().any(|c| c.stairs_up || c.stairs_down);

    let form = if has_stairs {
        "Stairwell"
    } else if walkable.len() > 14 {
        "Hall"
    } else if walkable.len() < 5 {
        "Alcove"
    } else {
        let (min_x, max_x) = walkable.iter().fold((0, 0), |(min, max), c| {
            (min.min(c.position.x), max.max(c.position.x))
        });
        let (min_y, max_y) = walkable.iter().fold((0, 0), |(min, max), c| {
            (min.min(c.position.y), max.max(c.position.y))
        });
        if ((max_x - min_x).abs() >= 4 && (max_y - min_y).abs() <= 2)
            || ((max_y - min_y).abs() >= 4 && (max_x - min_x).abs() <= 2)
        {
            "Passage"
        } else {
            "Chamber"
        }
    };

    const EPITHETS: &[&str] = &[
        "quiet", "dim", "shadowed", "cold", "drafty", "dusty", "still", "echoing",
    ];
    let epithet = EPITHETS[(seed as usize) % EPITHETS.len()];
    let full_form = format!("{material_prefix} {form}");

    let sensory = sensory_atmosphere(state).unwrap_or_else(|| "The air is cool and still.".into());

    let floor_phrase = floor_mat.map_or_else(String::new, |m| format!(" with a {} floor", safe(m)));

    format!("You stand in a {epithet} {full_form}{floor_phrase}.{walls_clause} {sensory}")
}

/// Synthesizes consistent sensory atmosphere for the setting.
pub fn sensory_atmosphere(state: &StateView) -> Option<String> {
    let key = current_place_key(state);
    let seed = key.map_or(0, key_hash);

    let o = &state.observation;
    let floor_mat = o
        .visible_cells
        .iter()
        .find(|c| distance(c.position) == 0 && !c.wall)
        .and_then(|c| floor_material(&o.visible_cells, c));
    let walls = surfaces::roles(&o.visible_cells).walls;

    if floor_mat.is_some_and(|m| m.contains("stone")) || walls.iter().any(|w| w.contains("stone")) {
        const STONE_SENSORY: &[&str] = &[
            "The air is cool and still, carrying a faint scent of ancient dust.",
            "A quiet chill lingers among the stones, where faint echoes answer your breath.",
            "Shadows pool softly in the corners of the masonry.",
            "A dry, still quiet hangs across the quarried rock.",
        ];
        Some(STONE_SENSORY[(seed as usize) % STONE_SENSORY.len()].into())
    } else if floor_mat.is_some_and(|m| m.contains("wood"))
        || walls.iter().any(|w| w.contains("wood"))
    {
        Some("The dry aroma of seasoned timber lingers in the enclosed space.".into())
    } else if floor_mat.is_some_and(|m| m.contains("dirt") || m.contains("earth")) {
        Some("The scent of cool earth hangs faintly in the air.".into())
    } else {
        Some("The air is cool and still.".into())
    }
}

/// Inspects environmental and sensory scenery.
pub fn examine_scenery(noun: &str, state: &StateView) -> Option<String> {
    let normalized = noun.trim().to_lowercase();
    match normalized.as_str() {
        "room" | "chamber" | "hall" | "passage" | "surroundings" | "area" | "place" | "here" => {
            Some(crate::adventure::describe(state))
        }
        "air" | "atmosphere" => sensory_atmosphere(state),
        "smell" | "scent" => {
            let o = &state.observation;
            let floor_mat = o
                .visible_cells
                .iter()
                .find(|c| distance(c.position) == 0 && !c.wall)
                .and_then(|c| floor_material(&o.visible_cells, c));
            let walls = surfaces::roles(&o.visible_cells).walls;
            if floor_mat.is_some_and(|m| m.contains("stone"))
                || walls.iter().any(|w| w.contains("stone"))
            {
                Some(
                    "The air smells cool and dry, with the faint, mineral scent of quarried stone."
                        .into(),
                )
            } else if floor_mat.is_some_and(|m| m.contains("dirt") || m.contains("earth")) {
                Some("The rich, damp aroma of earth and loam fills the air.".into())
            } else if floor_mat.is_some_and(|m| m.contains("wood")) {
                Some("You detect the faint, dry scent of aged timber.".into())
            } else {
                Some("The air carries no distinct scent.".into())
            }
        }
        "sound" | "noise" => {
            let o = &state.observation;
            if o.combat.is_some() {
                Some("You hear the tense, chaotic sounds of combat around you!".into())
            } else if !o.visible_actors.is_empty() {
                Some("You hear the faint scuffle and breathing of creatures nearby.".into())
            } else {
                Some("You listen closely. Aside from the faint whisper of air across the stone, all is quiet.".into())
            }
        }
        "stairs" | "stairway" | "steps" => {
            let stairs: Vec<_> = state
                .observation
                .visible_cells
                .iter()
                .filter(|c| c.stairs_up || c.stairs_down)
                .collect();
            if stairs.is_empty() {
                Some("You cannot see any stairs here.".into())
            } else {
                let up = stairs.iter().any(|c| c.stairs_up);
                let down = stairs.iter().any(|c| c.stairs_down);
                if up && down {
                    Some("Stone steps lead both up and down from here.".into())
                } else if up {
                    Some("A flight of stone steps leads upward.".into())
                } else {
                    Some("A flight of stone steps leads downward.".into())
                }
            }
        }
        _ => None,
    }
}

/// Searches the immediate room / floor.
pub fn search(state: &StateView) -> String {
    let items: Vec<_> = state
        .observation
        .ground_items
        .iter()
        .filter(|g| g.position.z == 0 && distance(g.position) <= 2)
        .map(|g| safe(&g.item.name))
        .collect();
    if items.is_empty() {
        "You search the surroundings carefully, but discover nothing else of interest.".into()
    } else {
        format!("Searching carefully reveals {}.", items.join(" and "))
    }
}

/// Assesses the player's physical condition, wounds, and active combat state.
pub fn diagnose(state: &StateView) -> String {
    if let Some(c) = &state.observation.combat {
        let condition = if c.dead {
            "You are dead."
        } else if c.hp == c.max_hp {
            "You are in peak physical condition, without a scratch."
        } else if c.hp >= (c.max_hp * 3) / 4 {
            "You have sustained minor cuts and bruises."
        } else if c.hp >= c.max_hp / 2 {
            "You are moderately wounded."
        } else if c.hp >= c.max_hp / 4 {
            "You are heavily wounded and bleeding noticeably."
        } else {
            "You are grievously wounded and on the verge of collapse."
        };
        let mut report = format!("{condition} (HP {}/{})", c.hp, c.max_hp);
        if let Some(ticks) = c.preparation_remaining {
            report.push_str(&format!(
                "\nYou are preparing an attack ({ticks} ticks remaining{}).",
                if c.preparation_active {
                    ""
                } else {
                    " - interrupted"
                }
            ));
        } else if c.recovery_remaining > 0 {
            report.push_str(&format!(
                "\nYou are recovering from your last exertion ({} ticks remaining).",
                c.recovery_remaining
            ));
        }
        report
    } else {
        "You are in good health, with no apparent injuries or afflictions.".into()
    }
}
