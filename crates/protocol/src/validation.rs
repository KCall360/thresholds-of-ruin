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
    InvalidLinkedHold,
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

impl crate::CombatView {
    /// Constant-sized progress constraints shared by full views and delta assembly.
    pub(crate) fn valid_sequence(&self) -> bool {
        self.sequence.as_ref().is_none_or(|sequence| {
            (2..=crate::MAX_SEQUENCE_STRIKES).contains(&sequence.total)
                && sequence.completed < sequence.total
                && self.preparation_remaining == Some(sequence.next_strike_remaining)
                && self.recovery_remaining == 0
                && if sequence.completed == 0 {
                    sequence.cancel_recovery_remaining == 0
                } else {
                    sequence.cancel_recovery_remaining >= sequence.next_strike_remaining
                }
                && !self.dead
        })
    }
}

impl StateView {
    /// Validate complete disclosed contents before publishing them to a client.
    /// Cell keys and entity identities may repeat through portals; occurrences
    /// are distinguished by observer-relative position. No world coordinates,
    /// portal graph, or hidden state are needed to check these invariants.
    pub fn validate(&self) -> Result<(), InvalidState> {
        let o = &self.observation;
        if o.linked_holds.len() > crate::MAX_LINKED_HOLDS
            || !unique(o.linked_holds.iter().map(|hold| hold.target))
            || o.linked_holds.iter().any(|hold| {
                hold.holder.is_some_and(|holder| {
                    holder == o.self_target
                        || !o.visible_actors.iter().any(|actor| actor.id == holder)
                })
            })
            || (o.combat.as_ref().is_some_and(|combat| combat.dead) && !o.linked_holds.is_empty())
        {
            return Err(InvalidState::InvalidLinkedHold);
        }
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
            .any(|item| item.quantity == 0 || !item.valid_disclosure())
        {
            return Err(InvalidState::InvalidItem);
        }
        if o.ground_items
            .iter()
            .any(|ground| ground.supply_reachable && ground.item.known_supply.is_none())
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
            let zero_hd = combat
                .own_stats
                .as_ref()
                .is_some_and(|stats| stats.hit_dice.total == 0);
            // Removing the final owned HD produces persistent death and zero
            // maximum Health. Legacy and positive-HD actors retain a positive max.
            (combat.max_hp == 0) != zero_hd
                || (zero_hd && !combat.dead)
                || combat.hp > combat.max_hp
                || combat.dead != (combat.hp == 0)
                || (combat.preparation_active && combat.preparation_remaining.is_none())
                || !combat.valid_sequence()
                || combat
                    .public_plan
                    .as_ref()
                    .is_some_and(|record| !record.valid_at(o.tick))
                || combat
                    .actors
                    .iter()
                    .any(|actor| actor.allied && actor.hostile)
                || combat.own_stats.as_ref().is_some_and(|stats| {
                    !stats.valid()
                        || !stats.nutrition.valid_life_state(combat.dead)
                        || (combat.dead && !stats.conditions.is_empty())
                        || ((combat.dead || combat.hp == combat.max_hp)
                            && stats.health_recovery.elapsed_ticks != 0)
                })
                || combat.events.iter().any(|event| match event {
                    crate::CombatEventView::Ability {
                        ability, strike, ..
                    } => {
                        !ability.valid()
                            || strike.is_some_and(|ordinal| ordinal >= crate::MAX_SEQUENCE_STRIKES)
                    }
                    crate::CombatEventView::Condition { condition } => !condition.valid(),
                    crate::CombatEventView::UnseenAbility { .. }
                    | crate::CombatEventView::Interrupted { .. }
                    | crate::CombatEventView::EscapeResolved { .. }
                    | crate::CombatEventView::Died { .. } => false,
                })
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
            if interactions.curse_blocked.len() > 4096
                || interactions.slots.len() > crate::MAX_EQUIPMENT_SLOTS
                || interactions.slots.iter().any(|slot| !slot.valid())
                || interactions.completed.iter().any(|action| {
                    !matches!(
                        action,
                        crate::Action::Equip { slot: 0..=63, .. }
                            | crate::Action::Unequip { .. }
                            | crate::Action::Drink { .. }
                            | crate::Action::Eat { .. }
                            | crate::Action::Read { .. }
                            | crate::Action::Zap { .. }
                    )
                })
                || interactions.identification.as_ref().is_some_and(|choice| {
                    choice.choice.0.is_empty()
                        || choice.choice.0.len() > 256
                        || choice.items.len() > 64
                        || !unique(choice.items.iter().copied())
                        || choice.items.iter().any(|item| {
                            inventory
                                .get(item)
                                .is_none_or(|item| item.class != choice.class)
                        })
                })
                || interactions.inventory.len() > inventory.len()
                || !unique(interactions.inventory.iter().map(|item| item.item))
                || interactions.inventory.iter().any(|interaction| {
                    let Some(item) = inventory.get(&interaction.item) else {
                        return true;
                    };
                    (!item.identified && interaction.known_equipment.is_some())
                        || (interaction.charges.is_some()
                            && (!item.identified || !interaction.zappable))
                        || interaction.slot.as_ref().is_some_and(|slot| !slot.valid())
                        || (interaction.slot.is_none()
                            && (interaction.equipped_slot.is_some()
                                || interaction.known_equipment.is_some()
                                || interaction.known_cursed.is_some()
                                || interaction.known_protected.is_some()))
                        || ((interaction.drinkable || interaction.edible)
                            && interaction.slot.is_some())
                        || (interaction.drinkable && interaction.edible)
                        || interaction.equipped_slot.is_some_and(|slot| {
                            !equipped.insert(slot)
                                || interactions.slots.get(usize::from(slot))
                                    != interaction.slot.as_ref()
                        })
                        || interaction
                            .known_equipment
                            .as_ref()
                            .is_some_and(|equipment| {
                                equipment.modifiers.len() > 128
                                    || equipment.modifiers.iter().any(|value| !value.valid())
                                    || equipment.attack.is_some()
                                        != interaction
                                            .slot
                                            .as_ref()
                                            .is_some_and(|slot| slot.requires_attack)
                                    || equipment
                                        .attack
                                        .as_ref()
                                        .is_some_and(|attack| !attack.valid())
                            })
                })
                || interactions.preparation.as_ref().is_some_and(|progress| {
                    match &progress.action {
                        crate::Action::Escape { .. } => false,
                        crate::Action::UseAbility { id, supplies, .. } => {
                            !crate::valid_content_id(id) || !supplies.valid()
                        }
                        crate::Action::Equip { item, slot } => {
                            !inventory.contains_key(item)
                                || usize::from(*slot) >= interactions.slots.len()
                        }
                        crate::Action::Unequip { item }
                        | crate::Action::Drink { item }
                        | crate::Action::Eat { item }
                        | crate::Action::Read { item }
                        | crate::Action::Zap { item, .. } => !inventory.contains_key(item),
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

impl crate::EquipmentSlot {
    fn valid(&self) -> bool {
        crate::valid_content_id(&self.id)
            && crate::valid_label(&self.name, 256)
            && crate::valid_label(&self.equipped_phrase, 256)
    }
}

impl crate::OwnStats {
    pub(crate) fn valid(&self) -> bool {
        let a = &self.attributes;
        let d = &self.defenses;
        // Hunger labels do not disclose the authored recovery policy. Validate
        // the actual personal Health clock without inferring a hidden rule.
        self.kind.valid()
            && self.binding.valid()
            && self.health_recovery.valid()
            && self.nutrition.valid()
            && self.restorable_losses.valid()
            && self.conditions.len() <= 128
            && self.conditions.iter().all(crate::PersonalCondition::valid)
            && self.hit_dice.valid()
            && self.subtypes.len() <= crate::MAX_CONTENT_REFERENCES
            && self.subtypes.iter().all(crate::ContentRef::valid)
            && unique(self.subtypes.iter().map(|value| &value.id))
            && [
                a.strength,
                a.speed,
                a.intellect,
                a.willpower,
                a.awareness,
                a.presence,
            ]
            .into_iter()
            .all(|value| value <= 1000)
            && [d.physical, d.cognitive, d.spiritual]
                .into_iter()
                .all(|value| (-1_000_000..=1_000_000).contains(&value))
            && self.skills.len() <= crate::MAX_CONTENT_REFERENCES
            && unique(self.skills.iter().map(|value| &value.skill.id))
            && self
                .skills
                .iter()
                .all(|value| value.skill.valid() && value.rank <= 5)
            && self.resources.len() <= crate::MAX_CONTENT_REFERENCES
            && unique(self.resources.iter().map(|value| &value.resource.id))
            && self.resources.iter().all(|value| {
                value.resource.valid()
                    && value.maximum <= 1_000_000
                    && value.balance <= value.maximum
                    && value.available.checked_add(value.reserved) == Some(value.balance)
            })
            && self.active_talents.len() + self.dormant_talents.len() <= crate::MAX_OWNED_TALENTS
            && self
                .active_talents
                .iter()
                .chain(&self.dormant_talents)
                .all(crate::ContentRef::valid)
            && unique(
                self.active_talents
                    .iter()
                    .chain(&self.dormant_talents)
                    .map(|value| &value.id),
            )
            && self.abilities.len() <= crate::MAX_CONTENT_REFERENCES
            && unique(self.abilities.iter().map(|value| &value.id))
            && self.abilities.iter().all(crate::AbilityView::valid)
            && self
                .abilities
                .iter()
                .filter(|value| value.basic_attack)
                .count()
                <= 1
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
