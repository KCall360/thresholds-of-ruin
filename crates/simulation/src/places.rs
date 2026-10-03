//! Character-owned mnemonic names, learned only at perception boundaries.
use crate::{ActorId, Game, GameError};
use tor_world::Location;

pub(crate) fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name == name.trim()
        && name.len() <= 80
        && !name.chars().any(char::is_control)
}

/// Where a remembered place's name came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameOrigin {
    /// A mnemonic the game made up when the place was first seen.
    Invented,
    /// The scenario's name for the place, learned on seeing it.
    Authored,
    /// The player's own name for it.
    Player,
}

/// A remembered place's name, and where it came from.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceName {
    pub name: String,
    pub origin: NameOrigin,
}

impl Game {
    pub fn remembered_places(
        &self,
        actor: ActorId,
    ) -> impl Iterator<Item = (Location, &str, NameOrigin)> {
        self.navigation
            .get(&actor)
            .into_iter()
            .flat_map(|n| n.places.iter())
            .map(|(location, place)| (*location, place.name.as_str(), place.origin))
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
        navigation.places.insert(
            location,
            PlaceName {
                name: name.into(),
                origin: NameOrigin::Player,
            },
        );
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
            // Every discovery takes an ordinal, named or not, so invented
            // names stay as they were.
            let name = match self.world.place_name(location) {
                Some(authored) => PlaceName {
                    name: authored.to_owned(),
                    origin: NameOrigin::Authored,
                },
                None => PlaceName {
                    name: mnemonic(self.seed, ordinal),
                    origin: NameOrigin::Invented,
                },
            };
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
    // Both words change from one discovery to the next, and the 256 pairs
    // are each used once per cycle.
    let index = ordinal.wrapping_add(seed) % 256;
    let (first, cycle) = (index % 16, index / 16);
    let base = format!(
        "{} {}",
        FIRST[first as usize],
        LAST[((first + cycle) % 16) as usize]
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
        // Regression: consecutive discoveries were all "... Promise".
        for ordinal in 0..255 {
            let (a, b) = (mnemonic(42, ordinal), mnemonic(42, ordinal + 1));
            let words = |n: &str| n.split(' ').map(str::to_owned).collect::<Vec<_>>();
            let (a, b) = (words(&a), words(&b));
            assert!(a[0] != b[0] && a[1] != b[1], "{a:?} {b:?}");
        }
    }
}
