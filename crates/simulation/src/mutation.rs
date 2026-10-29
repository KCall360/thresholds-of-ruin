//! Private, bounded mutation evidence for derived-cache validation.
//!
//! Stamps and ancestry are process-local witnesses, never gameplay ordering,
//! persisted state, or value equality. Losing evidence means a cold rebuild.
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tor_world::Shared;

const MAX_DEPTH: usize = 32;
const MAX_NODE_KEYS: usize = 1024;
static NEXT_STAMP: AtomicU64 = AtomicU64::new(1);

/// Opaque allocation identity; equal values certify the same publication only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MutationStamp(u64);

fn allocate_stamp(next: &AtomicU64) -> Option<MutationStamp> {
    let mut current = next.load(Ordering::Relaxed);
    loop {
        let following = current.checked_add(1)?;
        match next.compare_exchange_weak(current, following, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return Some(MutationStamp(current)),
            Err(observed) => current = observed,
        }
    }
}

/// Compose changes in chronological order, retaining any earliest-before and
/// latest-after facts required by the consumer's exact invalidation contract.
pub(crate) trait MutationChange: Clone {
    fn append(&mut self, later: &Self);
}

struct Node<K, D> {
    parent: Option<Arc<Node<K, D>>>,
    depth: usize,
    changes: BTreeMap<K, D>,
}

impl<K, D> Node<K, D> {
    fn root() -> Self {
        Self {
            parent: None,
            depth: 1,
            changes: BTreeMap::new(),
        }
    }
}

/// Holding a cursor freezes its node. Pointer identity cannot be recycled while
/// this owning handle remains alive; no actor or item payload is retained.
pub(crate) struct MutationCursor<K, D> {
    node: Arc<Node<K, D>>,
}

impl<K, D> Clone for MutationCursor<K, D> {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
        }
    }
}

pub(crate) struct MutationLedger<K: Ord + Copy, D: MutationChange> {
    stamps: Shared<BTreeMap<K, MutationStamp>>,
    node: Arc<Node<K, D>>,
}

impl<K: Ord + Copy, D: MutationChange> Clone for MutationLedger<K, D> {
    fn clone(&self) -> Self {
        Self {
            stamps: self.stamps.clone(),
            node: self.node.clone(),
        }
    }
}

impl<K: Ord + Copy, D: MutationChange> Default for MutationLedger<K, D> {
    fn default() -> Self {
        Self {
            stamps: Shared::new(BTreeMap::new()),
            node: Arc::new(Node::root()),
        }
    }
}

impl<K: Ord + Copy, D: MutationChange> MutationLedger<K, D> {
    pub(crate) fn publish(&mut self, id: K, change: D, present: bool) {
        crate::diagnostics::mutation_publication();
        let stamp = present.then(|| allocate_stamp(&NEXT_STAMP)).flatten();
        if let Some(stamp) = stamp {
            self.stamps.insert(id, stamp);
        } else {
            // A removed entity or exhausted allocator cannot retain a stale
            // certificate. Future consumers must use their unknown path.
            self.stamps.remove(&id);
        }

        if Arc::get_mut(&mut self.node).is_none() {
            self.node = if self.node.depth < MAX_DEPTH {
                Arc::new(Node {
                    parent: Some(self.node.clone()),
                    depth: self.node.depth + 1,
                    changes: BTreeMap::new(),
                })
            } else {
                Arc::new(Node::root())
            };
        }
        // A unique node has no captured cursor at its current state. Once its
        // bounded change set is full, drop older ancestry rather than claiming
        // an incomplete delta for any retained ancestor.
        if self.node.changes.len() >= MAX_NODE_KEYS && !self.node.changes.contains_key(&id) {
            self.node = Arc::new(Node::root());
        }
        let node = Arc::get_mut(&mut self.node).expect("unshared mutation node");
        match node.changes.entry(id) {
            std::collections::btree_map::Entry::Occupied(mut old) => old.get_mut().append(&change),
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(change);
            }
        }
    }

    pub(crate) fn stamp(&self, id: &K) -> Option<MutationStamp> {
        self.stamps.get(id).copied()
    }

    pub(crate) fn cursor(&self) -> MutationCursor<K, D> {
        MutationCursor {
            node: self.node.clone(),
        }
    }

    /// True only when the exact captured state is still in this branch's
    /// retained ancestry, including the unchanged current state.
    pub(crate) fn certifies_since(&self, cursor: &MutationCursor<K, D>) -> bool {
        let mut node = &self.node;
        loop {
            if Arc::ptr_eq(node, &cursor.node) {
                return true;
            }
            let Some(parent) = &node.parent else {
                return false;
            };
            node = parent;
        }
    }

    /// Ordered keys, with each change composed oldest-to-newest. Unknown,
    /// unrelated, forked-away and pruned evidence all return None, not empty.
    pub(crate) fn changes_since(&self, cursor: &MutationCursor<K, D>) -> Option<BTreeMap<K, D>> {
        let mut path = Vec::new();
        let mut node = &self.node;
        while !Arc::ptr_eq(node, &cursor.node) {
            path.push(node);
            node = node.parent.as_ref()?;
        }
        let mut changes: BTreeMap<K, D> = BTreeMap::new();
        for node in path.into_iter().rev() {
            for (&id, change) in &node.changes {
                match changes.entry(id) {
                    std::collections::btree_map::Entry::Occupied(mut old) => {
                        old.get_mut().append(change);
                    }
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        slot.insert(change.clone());
                    }
                }
            }
        }
        Some(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Change {
        before: i32,
        after: i32,
        publications: usize,
    }

    impl MutationChange for Change {
        fn append(&mut self, later: &Self) {
            self.after = later.after;
            self.publications += later.publications;
        }
    }

    fn change(before: i32, after: i32) -> Change {
        Change {
            before,
            after,
            publications: 1,
        }
    }

    #[test]
    fn clone_forks_keep_independent_stamps_and_ancestry() {
        let mut original = MutationLedger::<u32, Change>::default();
        original.publish(1, change(0, 1), true);
        let shared = original.cursor();
        let first_stamp = original.stamp(&1).unwrap();
        let mut fork = original.clone();
        fork.publish(1, change(1, 2), true);
        let fork_cursor = fork.cursor();
        original.publish(2, change(0, 3), true);
        assert_eq!(original.stamp(&1), Some(first_stamp));
        assert_ne!(fork.stamp(&1), Some(first_stamp));
        assert_eq!(fork.stamp(&2), None);
        assert!(original.certifies_since(&shared));
        assert!(fork.certifies_since(&shared));
        assert!(!original.certifies_since(&fork_cursor));
        assert_eq!(original.changes_since(&fork_cursor), None);
        assert_eq!(
            original.changes_since(&shared),
            Some(BTreeMap::from([(2, change(0, 3))]))
        );
        assert_eq!(
            fork.changes_since(&shared),
            Some(BTreeMap::from([(1, change(1, 2))]))
        );
    }

    #[test]
    fn publication_stamps_detect_aba_removal_and_reinsertion() {
        let mut ledger = MutationLedger::<u32, Change>::default();
        assert_eq!(ledger.stamp(&1), None);
        ledger.publish(1, change(0, 5), true);
        let first = ledger.stamp(&1).unwrap();
        ledger.publish(1, change(5, 8), true);
        let second = ledger.stamp(&1).unwrap();
        ledger.publish(1, change(8, 5), true);
        let third = ledger.stamp(&1).unwrap();
        assert_ne!(first, second);
        assert_ne!(first, third);
        assert_ne!(second, third);
        ledger.publish(1, change(5, 0), false);
        assert_eq!(ledger.stamp(&1), None);
        ledger.publish(1, change(0, 5), true);
        assert_ne!(ledger.stamp(&1), Some(first));
        assert_ne!(ledger.stamp(&1), Some(third));
    }

    #[test]
    fn deltas_coalesce_in_chronological_order_across_frozen_nodes() {
        let mut ledger = MutationLedger::<u32, Change>::default();
        let start = ledger.cursor();
        assert_eq!(ledger.changes_since(&start), Some(BTreeMap::new()));
        ledger.publish(2, change(4, 5), true);
        ledger.publish(2, change(5, 6), true);
        let middle = ledger.cursor();
        ledger.publish(1, change(0, 9), true);
        ledger.publish(2, change(6, 4), true);
        assert_eq!(
            ledger.changes_since(&start),
            Some(BTreeMap::from([
                (1, change(0, 9)),
                (
                    2,
                    Change {
                        before: 4,
                        after: 4,
                        publications: 3
                    }
                ),
            ]))
        );
        assert_eq!(
            ledger.changes_since(&middle),
            Some(BTreeMap::from([(1, change(0, 9)), (2, change(6, 4)),]))
        );
        let unrelated = MutationLedger::<u32, Change>::default().cursor();
        assert!(!ledger.certifies_since(&unrelated));
        assert_eq!(ledger.changes_since(&unrelated), None);
        assert_eq!(
            ledger.changes_since(&ledger.cursor()),
            Some(BTreeMap::new())
        );
    }

    #[test]
    fn ancestry_depth_prunes_to_unknown_without_losing_current_stamps() {
        let mut ledger = MutationLedger::<u32, Change>::default();
        let oldest = ledger.cursor();
        let mut retained = Vec::new();
        for value in 0..MAX_DEPTH {
            retained.push(ledger.cursor());
            ledger.publish(1, change(value as i32, value as i32 + 1), true);
            assert!(ledger.node.depth <= MAX_DEPTH);
        }
        assert!(!ledger.certifies_since(&oldest));
        assert_eq!(ledger.changes_since(&oldest), None);
        assert!(ledger.stamp(&1).is_some());
        let current = ledger.cursor();
        ledger.publish(1, change(32, 33), true);
        assert_eq!(
            ledger.changes_since(&current),
            Some(BTreeMap::from([(1, change(32, 33))]))
        );
    }

    #[test]
    fn node_key_bound_prunes_ancestry_but_not_entity_stamps() {
        let mut ledger = MutationLedger::<usize, Change>::default();
        let start = ledger.cursor();
        for id in 0..MAX_NODE_KEYS {
            ledger.publish(id, change(0, 1), true);
        }
        assert!(ledger.certifies_since(&start));
        assert_eq!(ledger.node.changes.len(), MAX_NODE_KEYS);
        ledger.publish(0, change(1, 2), true);
        assert!(
            ledger.certifies_since(&start),
            "coalescing an existing key does not overflow"
        );
        ledger.publish(MAX_NODE_KEYS, change(0, 1), true);
        assert!(!ledger.certifies_since(&start));
        assert_eq!(ledger.changes_since(&start), None);
        assert_eq!(ledger.node.changes.len(), 1);
        assert!(ledger.stamp(&0).is_some());
        assert!(ledger.stamp(&MAX_NODE_KEYS).is_some());
    }

    #[test]
    fn allocator_exhaustion_never_wraps_or_reuses_identity() {
        let next = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_stamp(&next), Some(MutationStamp(u64::MAX - 1)));
        assert_eq!(allocate_stamp(&next), None);
        assert_eq!(allocate_stamp(&next), None);
        assert_eq!(next.load(Ordering::Relaxed), u64::MAX);
    }
}
