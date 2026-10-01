//! Inventory letters stick to item ids. They are not sent.

use std::collections::{BTreeMap, BTreeSet};

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InventoryLetters {
    by_id: BTreeMap<u64, char>,
}

impl InventoryLetters {
    pub fn get(&self, id: u64) -> Option<char> {
        self.by_id.get(&id).copied()
    }

    pub fn pairs(&self) -> impl Iterator<Item = (u64, char)> + '_ {
        self.by_id.iter().map(|(&id, &letter)| (id, letter))
    }

    pub fn id_for(&self, letter: char) -> Option<u64> {
        self.by_id
            .iter()
            .find(|(_, assigned)| **assigned == letter)
            .map(|(&id, _)| id)
    }

    /// A new snapshot or a branch change assigns letters in ascending id order.
    pub fn rebuild(&mut self, ids: &[u64]) {
        self.by_id.clear();
        self.assign_new(ids);
    }

    /// A same-branch snapshot keeps letters for ids that are still carried.
    pub fn retain(&mut self, ids: &[u64]) {
        let kept: BTreeSet<u64> = ids.iter().copied().collect();
        self.by_id.retain(|id, _| kept.contains(id));
        self.assign_new(ids);
    }

    /// Ordinary update. `take_or_drop` is the merge case from the items guide:
    /// one previously lettered id left and one new id appeared.
    pub fn update(&mut self, after: &[u64], take_or_drop: bool) {
        let before: BTreeSet<u64> = self.by_id.keys().copied().collect();
        let after_set: BTreeSet<u64> = after.iter().copied().collect();
        let mut left: Vec<u64> = before.difference(&after_set).copied().collect();
        let mut arrived: Vec<u64> = after_set.difference(&before).copied().collect();
        left.sort_unstable();
        arrived.sort_unstable();
        if take_or_drop && left.len() == 1 && arrived.len() == 1 {
            if let Some(letter) = self.by_id.remove(&left[0]) {
                self.by_id.insert(arrived[0], letter);
            }
            arrived.clear();
        }
        self.by_id.retain(|id, _| after_set.contains(id));
        self.assign_new(&arrived);
    }

    /// Swap two assigned letters, or move one letter onto a free one.
    pub fn adjust(&mut self, first: char, second: char) -> Result<(), &'static str> {
        if !is_inventory_letter(first) || !is_inventory_letter(second) {
            return Err("That isn't an inventory letter.");
        }
        if first == second {
            return Ok(());
        }
        match (self.id_for(first), self.id_for(second)) {
            (Some(left), Some(right)) => {
                self.by_id.insert(left, second);
                self.by_id.insert(right, first);
                Ok(())
            }
            (Some(id), None) => {
                self.by_id.insert(id, second);
                Ok(())
            }
            (None, Some(id)) => {
                self.by_id.insert(id, first);
                Ok(())
            }
            (None, None) => Err("Neither letter is in use."),
        }
    }

    fn assign_new(&mut self, ids: &[u64]) {
        let mut fresh: Vec<u64> = ids
            .iter()
            .copied()
            .filter(|id| !self.by_id.contains_key(id))
            .collect();
        fresh.sort_unstable();
        fresh.dedup();
        for id in fresh {
            let Some(letter) = self.lowest_free() else {
                break;
            };
            self.by_id.insert(id, letter);
        }
    }

    fn lowest_free(&self) -> Option<char> {
        ALPHABET
            .iter()
            .map(|byte| *byte as char)
            .find(|letter| self.id_for(*letter).is_none())
    }
}

pub fn is_inventory_letter(letter: char) -> bool {
    ALPHABET.contains(&(letter as u8))
}

pub fn temporary_letter(index: usize) -> Option<char> {
    ALPHABET.get(index).map(|byte| *byte as char)
}
