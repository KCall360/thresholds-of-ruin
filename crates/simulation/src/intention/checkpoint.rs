//! Saved queue structure is separate from runtime ownership. Complete intention
//! values are pooled across boundaries; references retain canonical actor order.
use super::{IntentionId, IntentionQueue, QueuedIntention, MAX_QUEUED_INTENTIONS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_world::Shared;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Snapshot {
    next_id: u64,
    entries: Vec<usize>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Pool {
    entries: Vec<QueuedIntention>,
    /// Derived capture accelerator, rebuilt after decoding. Identity narrows
    /// candidates; sharing always compares the complete immutable value.
    #[serde(skip)]
    candidates: BTreeMap<IntentionId, Vec<usize>>,
}

impl Pool {
    pub(crate) fn capture(&mut self, queue: &IntentionQueue) -> Snapshot {
        if self.candidates.is_empty() && !self.entries.is_empty() {
            for (index, entry) in self.entries.iter().enumerate() {
                self.candidates.entry(entry.id).or_default().push(index);
            }
        }
        let mut references = Vec::with_capacity(queue.entries.len());
        for entry in queue.entries.values() {
            let candidates = self.candidates.entry(entry.id).or_default();
            let index = candidates
                .iter()
                .copied()
                .find(|&index| self.entries[index] == *entry)
                .unwrap_or_else(|| {
                    let index = self.entries.len();
                    self.entries.push(entry.clone());
                    candidates.push(index);
                    index
                });
            references.push(index);
        }
        Snapshot {
            next_id: queue.next_id,
            entries: references,
        }
    }
}

/// Restore identical queues once. Games keep their copy-on-write ownership;
/// this temporary cache disappears after checkpoint restoration.
#[derive(Default)]
pub(crate) struct Restore {
    queues: BTreeMap<Snapshot, Shared<IntentionQueue>>,
}

impl Restore {
    pub(crate) fn restore(
        &mut self,
        snapshot: Snapshot,
        pool: &Pool,
    ) -> Option<Shared<IntentionQueue>> {
        if snapshot.entries.len() > MAX_QUEUED_INTENTIONS {
            return None;
        }
        if let Some(queue) = self.queues.get(&snapshot) {
            return Some(queue.clone());
        }
        let mut entries = BTreeMap::new();
        let mut previous = None;
        for &index in &snapshot.entries {
            let entry = pool.entries.get(index)?;
            if previous.is_some_and(|actor| actor >= entry.actor) {
                return None;
            }
            previous = Some(entry.actor);
            entries.insert(entry.actor, entry.clone());
        }
        let queue = Shared::new(IntentionQueue {
            next_id: snapshot.next_id,
            entries,
        });
        self.queues.insert(snapshot, queue.clone());
        Some(queue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intention::{IntentionOrigin, IntentionState, IntentionWork};
    use crate::{Action, ActorId};

    fn entry(id: u64, actor: u64) -> QueuedIntention {
        QueuedIntention {
            id: IntentionId(id),
            actor: ActorId(actor),
            work: IntentionWork::Action(Action::Wait),
            origin: IntentionOrigin::Human,
            state: IntentionState::Queued,
            movement_context: None,
        }
    }

    #[test]
    fn pool_compares_complete_values_and_rebuilds_its_capture_index_after_decode() {
        let mut queue = IntentionQueue {
            next_id: 3,
            entries: BTreeMap::from([(ActorId(1), entry(1, 1)), (ActorId(2), entry(2, 2))]),
        };
        let mut pool = Pool::default();
        let original = pool.capture(&queue);
        queue.entries.get_mut(&ActorId(1)).unwrap().state = IntentionState::Suspended;
        let suspended = pool.capture(&queue);
        assert_ne!(original.entries, suspended.entries);
        assert_eq!(pool.entries.len(), 3);
        let bytes = serde_json::to_vec(&pool).unwrap();
        let mut decoded: Pool = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.capture(&queue).entries, suspended.entries);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
        queue.entries.get_mut(&ActorId(1)).unwrap().work =
            IntentionWork::ResumeAttack { target: ActorId(2) };
        pool.capture(&queue);
        assert_eq!(pool.entries.len(), 4);
    }

    #[test]
    fn restore_rejects_invalid_duplicate_unordered_and_oversized_references() {
        let pool = Pool {
            entries: vec![entry(1, 1), entry(2, 2)],
            ..Pool::default()
        };
        for entries in [
            vec![2],
            vec![0, 0],
            vec![1, 0],
            vec![0; MAX_QUEUED_INTENTIONS + 1],
        ] {
            assert!(Restore::default()
                .restore(
                    Snapshot {
                        next_id: 3,
                        entries
                    },
                    &pool
                )
                .is_none());
        }
        let snapshot = Snapshot {
            next_id: 3,
            entries: vec![0, 1],
        };
        let mut restore = Restore::default();
        let first = restore.restore(snapshot.clone(), &pool).unwrap();
        let second = restore.restore(snapshot, &pool).unwrap();
        assert!(first.shares_storage(&second));
    }
}
