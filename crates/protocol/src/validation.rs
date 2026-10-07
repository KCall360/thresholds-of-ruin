//! Structural invariants of a disclosed state, independent of world topology.
use crate::StateView;
use std::collections::BTreeSet;

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
}

// Canonically ordered collections need no allocation. Unordered full views
// remain valid, but still require distinct occurrences.
fn unique<T: Ord>(values: impl Iterator<Item = T> + Clone) -> bool {
    let mut previous = None;
    for value in values.clone() {
        if previous.as_ref().is_some_and(|before| before >= &value) {
            let mut seen = BTreeSet::new();
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
            let carried: BTreeSet<_> = o.inventory.iter().map(|item| item.id).collect();
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
        if !matches!(
            crate::codec::encoded_length(self, crate::MAX_STATE_BYTES),
            Ok(Some(_))
        ) {
            return Err(InvalidState::StateTooLarge);
        }
        Ok(())
    }
}
