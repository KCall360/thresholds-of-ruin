//! Character-owned mnemonic names, learned only at perception boundaries.
use crate::{ActorId, Game, GameError};
use tor_world::Location;

pub(crate) fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name == name.trim()
        && name.len() <= 80
        && !name.chars().any(char::is_control)
}

impl Game {
    pub fn remembered_places(&self, actor: ActorId) -> impl Iterator<Item = (Location, &str)> {
        self.navigation
            .get(&actor)
            .into_iter()
            .flat_map(|n| n.places.iter())
            .map(|(location, name)| (*location, name.as_str()))
    }

    /// Naming is free, and cannot establish knowledge of an undiscovered cell.
    pub fn rename_place(
        &mut self,
        actor: ActorId,
        location: Location,
        name: &str,
    ) -> Result<(), GameError> {
        if !valid_name(name) {
            return Err(GameError::InvalidLocation);
        }
        let navigation = self
            .navigation
            .get_mut(&actor)
            .ok_or(GameError::UnknownActor)?;
        if !navigation.places.contains_key(&location) {
            return Err(GameError::InvalidLocation);
        }
        navigation.places.insert(location, name.into());
        Ok(())
    }

    pub(crate) fn refresh_places(&mut self, actor: ActorId, scene: &[tor_world::SightCell]) {
        let navigation = self.navigation.entry(actor).or_default();
        // Stable order and cell identity deduplicate repeated portal occurrences.
        let discovered: std::collections::BTreeSet<_> = scene
            .iter()
            .filter(|cell| {
                !cell.wall
                    && self.world.has_place_hint(cell.location)
                    && !navigation.places.contains_key(&cell.location)
            })
            .map(|cell| cell.location)
            .collect();
        if discovered.is_empty() {
            return;
        }
        let first = navigation.places.keys().count() as u64;
        for (ordinal, location) in (first..).zip(discovered) {
            let name = mnemonic(self.seed, ordinal);
            navigation.places.insert(location, name);
        }
    }
}

// These are imagined mnemonics, never claims about materials, inhabitants or
// region membership. Only the seed and this character's discovery order matter.
fn mnemonic(seed: u64, ordinal: u64) -> String {
    const FIRST: [&str; 16] = [
        "Amber",
        "Ashen",
        "Silver",
        "Velvet",
        "Wandering",
        "Forgotten",
        "Quiet",
        "Twilight",
        "Distant",
        "Fading",
        "Hollow",
        "Hidden",
        "Patient",
        "Crimson",
        "Pale",
        "Midnight",
    ];
    const LAST: [&str; 16] = [
        "Reverie", "Echo", "Promise", "Refrain", "Memory", "Vigil", "Whisper", "Dream", "Solace",
        "Lament", "Wish", "Omen", "Riddle", "Lullaby", "Longing", "Respite",
    ];
    let index = ordinal.wrapping_add(seed) % 256;
    let base = format!(
        "{} {}",
        FIRST[(index % 16) as usize],
        LAST[(index / 16) as usize]
    );
    if ordinal < 256 {
        base
    } else {
        format!("{base} {}", ordinal / 256 + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Position, RegionId};

    #[test]
    fn unchanged_perception_preserves_shared_knowledge_and_names_scale_without_collision() {
        let mut game = Game::two_room_with_place_hints(42);
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.refresh_navigation();
        let before = game.clone();
        game.refresh_navigation();
        assert!(game.navigation[&actor].shares_storage(&before.navigation[&actor]));
        let names: std::collections::BTreeSet<_> = (0..1024).map(|i| mnemonic(42, i)).collect();
        assert_eq!(names.len(), 1024);
        assert!(names.iter().all(|name| valid_name(name)));
    }
}
