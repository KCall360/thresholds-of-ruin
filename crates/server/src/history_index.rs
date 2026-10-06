//! Derived journal lookup and privacy-scoped pagination. The archive remains
//! authoritative; positions are stable because retained records are append-only.
use crate::journal::JournalEntry;
use std::collections::BTreeMap;
use tor_protocol::{ActorId, Audience, Author, BranchId, EntryId};

#[derive(Clone, Debug, Default)]
struct Scope {
    public: Vec<usize>,
    private: BTreeMap<String, Vec<usize>>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct HistoryIndex {
    entries: BTreeMap<EntryId, usize>,
    intentions: BTreeMap<tor_simulation::IntentionId, usize>,
    resolutions: BTreeMap<(String, tor_simulation::IntentionId), usize>,
    ends: BTreeMap<(String, tor_simulation::IntentionId), usize>,
    branches: BTreeMap<String, BTreeMap<ActorId, Scope>>,
}

impl HistoryIndex {
    pub fn rebuild<'a>(entries: impl IntoIterator<Item = &'a JournalEntry>) -> Self {
        let mut index = Self::default();
        for (position, entry) in entries.into_iter().enumerate() {
            index.append(entry, position);
        }
        index
    }

    pub fn append(&mut self, entry: &JournalEntry, position: usize) {
        assert_eq!(
            position,
            self.entries.len(),
            "append-only journal positions"
        );
        assert!(
            !self.entries.contains_key(&entry.id),
            "unique journal identity"
        );
        self.entries.insert(entry.id.clone(), position);
        for end in &entry.intention_ends {
            self.ends
                .insert((entry.branch.0.clone(), end.intention), position);
        }
        if let crate::journal::JournalContent::IntentionStarted { intention, .. }
        | crate::journal::JournalContent::IntentionFailed { intention, .. }
        | crate::journal::JournalContent::IntentionContinued { intention, .. }
        | crate::journal::JournalContent::IntentionContinuationFailed { intention, .. } =
            entry.content
        {
            self.resolutions
                .insert((entry.branch.0.clone(), intention), position);
        }
        if let crate::journal::JournalContent::IntentionAdmitted { intention, .. } = entry.content {
            assert!(
                self.intentions.insert(intention, position).is_none(),
                "unique intention identity"
            );
            return;
        }
        if matches!(
            entry.content,
            crate::journal::JournalContent::IntentionFailed { .. }
                | crate::journal::JournalContent::IntentionContinuationFailed { .. }
                | crate::journal::JournalContent::IntentionChanged { .. }
        ) {
            return;
        }
        let scope = self
            .branches
            .entry(entry.branch.0.clone())
            .or_default()
            .entry(entry.actor)
            .or_default();
        if entry.audience == Audience::Actor {
            scope.public.push(position);
        } else if let Author::User { user } | Author::Frontend { user, .. } = &entry.author {
            scope
                .private
                .entry(user.clone())
                .or_default()
                .push(position);
        }
    }

    pub fn find(&self, id: &EntryId) -> Option<usize> {
        self.entries.get(id).copied()
    }

    pub fn intention_admission(&self, id: tor_simulation::IntentionId) -> Option<usize> {
        self.intentions.get(&id).copied()
    }

    pub fn intention_resolution(
        &self,
        branch: &BranchId,
        id: tor_simulation::IntentionId,
    ) -> Option<usize> {
        self.resolutions.get(&(branch.0.clone(), id)).copied()
    }

    pub fn intention_end(
        &self,
        branch: &BranchId,
        id: tor_simulation::IntentionId,
    ) -> Option<usize> {
        self.ends.get(&(branch.0.clone(), id)).copied()
    }

    /// Returns chronological positions plus whether an older visible page exists.
    /// A validated anchor supplies its exclusive archive position. Filtering is
    /// established by the scope, before either binary search or pagination.
    pub fn page(
        &self,
        actor: ActorId,
        user: &str,
        branch: &BranchId,
        end: usize,
        limit: usize,
    ) -> (Vec<usize>, bool) {
        let Some(scope) = self
            .branches
            .get(&branch.0)
            .and_then(|actors| actors.get(&actor))
        else {
            return (Vec::new(), false);
        };
        let private = scope.private.get(user).map(Vec::as_slice).unwrap_or(&[]);
        let mut public = scope.public[..scope.public.partition_point(|&position| position < end)]
            .iter()
            .rev()
            .peekable();
        let mut private = private[..private.partition_point(|&position| position < end)]
            .iter()
            .rev()
            .peekable();
        let mut positions = Vec::with_capacity(limit.min(scope.public.len() + private.len()));
        while positions.len() < limit {
            let next = match (public.peek(), private.peek()) {
                (Some(a), Some(b)) if a >= b => public.next(),
                (Some(_), Some(_)) => private.next(),
                (Some(_), None) => public.next(),
                (None, Some(_)) => private.next(),
                (None, None) => break,
            };
            positions.push(*next.expect("selected nonempty history bucket"));
        }
        let older = public.peek().is_some() || private.peek().is_some();
        positions.reverse();
        (positions, older)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::JournalContent;
    use tor_protocol::{Anchor, AnnotationCategory};

    #[test]
    fn indexed_pages_match_reference_filter_for_every_scope_and_anchor() {
        let authors = [
            Author::User {
                user: "alice".into(),
            },
            Author::User { user: "bob".into() },
            Author::Frontend {
                user: "alice".into(),
                component: "text".into(),
            },
            Author::Backend {
                component: "simulation".into(),
            },
        ];
        let mut entries = Vec::new();
        // Interleave actors, branches and privacy rather than grouping their records.
        for round in 0..3 {
            for author in &authors {
                for audience in [Audience::Actor, Audience::Private] {
                    for branch in ["original", "rewound"] {
                        for actor in [ActorId(1), ActorId(2)] {
                            entries.push(JournalEntry {
                                intention_suspensions: Vec::new(),
                                intention_ends: Vec::new(),
                                id: EntryId(format!("entry-{}", entries.len())),
                                branch: BranchId(branch.into()),
                                actor,
                                tick: round,
                                author: author.clone(),
                                audience,
                                content: JournalContent::Annotation {
                                    anchor: Anchor::State { revision: 0 },
                                    category: AnnotationCategory::Note,
                                    text: "note".into(),
                                },
                            });
                        }
                    }
                }
            }
        }
        let rebuilt = HistoryIndex::rebuild(&entries);
        let mut appended = HistoryIndex::default();
        for (position, entry) in entries.iter().enumerate() {
            appended.append(entry, position);
        }
        for index in [&rebuilt, &appended, &rebuilt.clone()] {
            for (position, entry) in entries.iter().enumerate() {
                assert_eq!(index.find(&entry.id), Some(position));
            }
            assert_eq!(index.find(&EntryId("missing".into())), None);
            for actor in [ActorId(1), ActorId(2), ActorId(99)] {
                for user in ["alice", "bob", "observer", ""] {
                    for branch in ["original", "rewound", "missing"] {
                        let branch = BranchId(branch.into());
                        for end in 0..=entries.len() {
                            let visible: Vec<_> = entries[..end]
                                .iter()
                                .enumerate()
                                .filter(|(_, entry)| {
                                    entry.branch == branch && entry.visible_to(actor, user)
                                })
                                .map(|(position, _)| position)
                                .collect();
                            for limit in [1, 2, 7, 100] {
                                let start = visible.len().saturating_sub(limit);
                                assert_eq!(
                                    index.page(actor, user, &branch, end, limit),
                                    (visible[start..].to_vec(), start > 0)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
