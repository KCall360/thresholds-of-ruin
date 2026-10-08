//! Structural invariants of a disclosed state, independent of world topology.
use crate::StateView;
use std::{collections::HashSet, hash::Hash};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidState {
    StateTooLarge,
    DuplicateCell,
    DuplicateInventoryItem,
    DuplicateGroundItem,
    DuplicateActor,
    DuplicatePlace,
    InvalidItem,
    ConflictingItemLocation,
    InvalidCombat,
    InvalidMotion,
    InvalidInteraction,
}

// Canonically ordered collections need no allocation. Unordered full views
// remain valid, but still require distinct occurrences. Hash tables are used
// only for membership; their iteration order never affects validation or state.
fn unique<T: Ord + Hash>(values: impl Iterator<Item = T> + Clone) -> bool {
    let mut previous = None;
    for value in values.clone() {
        if previous.as_ref().is_some_and(|before| before >= &value) {
            let mut seen = HashSet::with_capacity(values.size_hint().0);
            return values.into_iter().all(|value| seen.insert(value));
        }
        previous = Some(value);
    }
    true
}

impl StateView {
    /// Validate complete disclosed contents before publishing them to a client.
    /// Cell keys and entity identities may repeat through portals; occurrences
    /// are distinguished by observer-relative position. No world coordinates,
    /// portal graph, or hidden state are needed to check these invariants.
    pub fn validate(&self) -> Result<(), InvalidState> {
        let o = &self.observation;
        if !unique(o.visible_cells.iter().map(|cell| cell.position)) {
            return Err(InvalidState::DuplicateCell);
        }
        if !unique(o.inventory.iter().map(|item| item.id)) {
            return Err(InvalidState::DuplicateInventoryItem);
        }
        if !unique(
            o.ground_items
                .iter()
                .map(|item| (item.position, item.item.id)),
        ) {
            return Err(InvalidState::DuplicateGroundItem);
        }
        if !unique(
            o.visible_actors
                .iter()
                .map(|actor| (actor.position, actor.id)),
        ) {
            return Err(InvalidState::DuplicateActor);
        }
        if !unique(o.places.iter().map(|place| place.key.as_str())) {
            return Err(InvalidState::DuplicatePlace);
        }
        if o.inventory
            .iter()
            .chain(o.ground_items.iter().map(|item| &item.item))
            .any(|item| item.quantity == 0)
        {
            return Err(InvalidState::InvalidItem);
        }
        if !o.inventory.is_empty() && !o.ground_items.is_empty() {
            let carried: HashSet<_> = o.inventory.iter().map(|item| item.id).collect();
            if o.ground_items
                .iter()
                .any(|item| carried.contains(&item.item.id))
            {
                return Err(InvalidState::ConflictingItemLocation);
            }
        }
        if o.combat.as_ref().is_some_and(|combat| {
            combat.max_hp == 0
                || combat.hp > combat.max_hp
                || combat.dead != (combat.hp == 0)
                || (combat.preparation_active && combat.preparation_remaining.is_none())
        }) {
            return Err(InvalidState::InvalidCombat);
        }
        if o.motion
            .as_ref()
            .is_some_and(|motion| motion.units_per_cell == 0)
        {
            return Err(InvalidState::InvalidMotion);
        }
        if let Some(interactions) = &o.interactions {
            let inventory: std::collections::HashMap<_, _> =
                o.inventory.iter().map(|item| (item.id, item)).collect();
            let mut equipped = HashSet::new();
            if interactions.slots.len() > 64
                || interactions.inventory.len() > inventory.len()
                || !unique(interactions.inventory.iter().map(|item| item.item))
                || interactions.inventory.iter().any(|interaction| {
                    let Some(item) = inventory.get(&interaction.item) else {
                        return true;
                    };
                    (!item.identified && interaction.known_equipment.is_some())
                        || (interaction.slot.is_none()
                            && (interaction.equipped_slot.is_some()
                                || interaction.known_equipment.is_some()))
                        || (interaction.drinkable && interaction.slot.is_some())
                        || interaction.equipped_slot.is_some_and(|slot| {
                            !equipped.insert(slot)
                                || interactions.slots.get(usize::from(slot)).copied()
                                    != interaction.slot
                        })
                        || interaction
                            .known_equipment
                            .as_ref()
                            .is_some_and(|equipment| {
                                !(-1000..=1000).contains(&equipment.defense)
                                    || equipment
                                        .reductions
                                        .values()
                                        .any(|amount| *amount > 1_000_000)
                                    || equipment.attack.as_ref().is_some_and(|attack| {
                                        interaction.slot != Some(crate::EquipmentSlot::Weapon)
                                            || !(-1000..=1000).contains(&attack.bonus)
                                            || attack.wind_up == 0
                                            || attack.wind_up.checked_add(attack.recovery).is_none()
                                            || attack.damage.is_empty()
                                            || attack
                                                .damage
                                                .values()
                                                .any(|amount| *amount > 1_000_000)
                                    })
                            })
                })
                || interactions.preparation.as_ref().is_some_and(|progress| {
                    match &progress.action {
                        crate::Action::Attack { .. } => false,
                        crate::Action::Equip { item, slot } => {
                            !inventory.contains_key(item)
                                || usize::from(*slot) >= interactions.slots.len()
                        }
                        crate::Action::Unequip { item } | crate::Action::Drink { item } => {
                            !inventory.contains_key(item)
                        }
                        _ => true,
                    }
                })
            {
                return Err(InvalidState::InvalidInteraction);
            }
        }
        if !matches!(
            crate::codec::encoded_length(self, crate::MAX_STATE_BYTES),
            Ok(Some(_))
        ) {
            return Err(InvalidState::StateTooLarge);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::unique;
    use crate::ItemTarget;
    use std::{
        cell::Cell,
        cmp::Ordering,
        hash::{Hash, Hasher},
    };

    #[derive(Clone, Copy)]
    struct CountedKey<'a> {
        id: ItemTarget,
        comparisons: &'a Cell<usize>,
        hashes: &'a Cell<usize>,
    }

    impl PartialEq for CountedKey<'_> {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
    impl Eq for CountedKey<'_> {}
    impl PartialOrd for CountedKey<'_> {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for CountedKey<'_> {
        fn cmp(&self, other: &Self) -> Ordering {
            self.comparisons.set(self.comparisons.get() + 1);
            self.id.cmp(&other.id)
        }
    }
    impl Hash for CountedKey<'_> {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.hashes.set(self.hashes.get() + 1);
            self.id.hash(state);
        }
    }

    fn keys<'a>(comparisons: &'a Cell<usize>, hashes: &'a Cell<usize>) -> Vec<CountedKey<'a>> {
        (0..1_000_u64)
            .map(|id| {
                let mut digest = [0; 32];
                digest[24..].copy_from_slice(&id.to_be_bytes());
                CountedKey {
                    id: ItemTarget::from_digest(digest),
                    comparisons,
                    hashes,
                }
            })
            .collect()
    }

    #[test]
    fn unordered_opaque_uniqueness_does_linear_ordering_work() {
        let comparisons = Cell::new(0);
        let hashes = Cell::new(0);
        let mut values = keys(&comparisons, &hashes);
        values.reverse();
        assert!(unique(values.iter().copied()));
        assert!(
            comparisons.get() <= values.len() * 2,
            "unordered identities repeated ordering work: {} comparisons for {} keys",
            comparisons.get(),
            values.len()
        );
        values.push(values[values.len() / 2]);
        assert!(!unique(values.iter().copied()));
    }

    #[test]
    fn canonical_opaque_uniqueness_keeps_its_hash_free_fast_path() {
        let comparisons = Cell::new(0);
        let hashes = Cell::new(0);
        let values = keys(&comparisons, &hashes);
        assert!(unique(values.iter().copied()));
        assert!(comparisons.get() <= values.len());
        assert_eq!(hashes.get(), 0);
    }
}
