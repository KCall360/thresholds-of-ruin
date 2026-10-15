//! Private checkpoint audit of exact entry receipts, including abandoned branches.
//! Journal replay compares each executed delta directly; checkpoint recovery must
//! prove the same ledger correspondence without replaying gameplay effects.
use super::*;
use tor_simulation::floor_activation::{
    EntryCause, PlacementDecision, StageResult, MAX_RETAINED_DECISIONS, MAX_SCOPES,
};
use tor_simulation::unique_retrieval::RetrievalResult;
use tor_simulation::ItemId;

struct LedgerNode<'a> {
    previous: Option<usize>,
    records: &'a [FloorActivationRecord],
    count: usize,
    decisions: usize,
}

#[derive(Clone, Copy, Default)]
struct RetrievalLedger<'a> {
    item: Option<ItemId>,
    entries: u64,
    placements: u64,
    latest: &'a [RetrievalEntry],
}

struct LedgerBoundary<'a> {
    id: Option<EntryId>,
    selectable: bool,
    tail: Option<usize>,
    tick: u64,
    retrieval: RetrievalLedger<'a>,
}

pub(super) struct ActivationAudit<'a> {
    validator: &'a Game,
    tick: u64,
    retrieval: RetrievalLedger<'a>,
    // An arena avoids recursive destruction and full-history copies at every
    // journal boundary. Only an actual activation appends a receipt slice.
    nodes: Vec<LedgerNode<'a>>,
    tail: Option<usize>,
    scopes: BTreeSet<&'a str>,
    // Allocation identities cannot be reused across branches. Include explicit
    // non-population births retained by the journal, without inferring origins
    // for merged/split items or entities whose birth evidence is unavailable.
    actor_origins: BTreeSet<u64>,
    item_origins: BTreeSet<u64>,
    boundaries: VecDeque<LedgerBoundary<'a>>,
}

impl<'a> ActivationAudit<'a> {
    pub(super) fn new(
        initial: &'a [FloorActivationRecord],
        initial_retrievals: &'a [RetrievalEntry],
        validator: &'a Game,
    ) -> Result<Self, Failure> {
        if initial_retrievals.len() > 1
            || initial_retrievals
                .iter()
                .any(|entry| entry.cause != EntryCause::Initial || entry.tick != 0)
            || initial.len() > 1
            || initial
                .iter()
                .any(|record| record.cause != EntryCause::Initial || record.tick != 0)
        {
            return Err(invalid_archive());
        }
        let mut audit = Self {
            validator,
            tick: 0,
            retrieval: RetrievalLedger::default(),
            nodes: Vec::new(),
            tail: None,
            scopes: BTreeSet::new(),
            actor_origins: BTreeSet::new(),
            item_origins: BTreeSet::new(),
            boundaries: VecDeque::new(),
        };
        audit.append(initial)?;
        audit.append_retrievals(initial_retrievals)?;
        audit.boundaries.push_back(LedgerBoundary {
            id: None,
            selectable: true,
            tail: audit.tail,
            tick: 0,
            retrieval: audit.retrieval,
        });
        Ok(audit)
    }

    fn append(&mut self, records: &'a [FloorActivationRecord]) -> Result<(), Failure> {
        if records.is_empty() {
            return Ok(());
        }
        let mut count = self.tail.map_or(0, |tail| self.nodes[tail].count);
        let mut decisions = self.tail.map_or(0, |tail| self.nodes[tail].decisions);
        let mut tick = self.tail.map_or(0, |tail| {
            self.nodes[tail]
                .records
                .last()
                .expect("nonempty receipt slice")
                .tick
        });
        for record in records {
            self.validator
                .validate_activation_record(record)
                .and_then(|()| {
                    self.validator
                        .validate_activation_record_identity_bounds(record)
                })
                .map_err(|_| invalid_archive())?;
            if record.ordinal != count as u64
                || record.tick < tick
                || !self.scopes.insert(&record.scope)
            {
                return Err(invalid_archive());
            }
            count = count.checked_add(1).ok_or_else(invalid_archive)?;
            for stage in &record.decisions {
                let attempts = match &stage.result {
                    StageResult::Skipped { .. } => 0,
                    StageResult::Evaluated { attempts, .. } => {
                        for attempt in attempts {
                            if let PlacementDecision::Placed { entity, .. } = attempt {
                                let unique = entity
                                    .actor_birth()
                                    .is_none_or(|id| self.actor_origins.insert(id.0))
                                    && entity
                                        .item_birth()
                                        .is_none_or(|id| self.item_origins.insert(id.0));
                                // Rewind preserves identity high-water marks. A
                                // different historical origin cannot reuse an ID.
                                if !unique {
                                    return Err(invalid_archive());
                                }
                            }
                        }
                        attempts.iter().map(PlacementDecision::decision_count).sum()
                    }
                };
                decisions = decisions
                    .checked_add(1 + attempts)
                    .ok_or_else(invalid_archive)?;
            }
            if count > MAX_SCOPES || decisions > MAX_RETAINED_DECISIONS {
                return Err(invalid_archive());
            }
            tick = record.tick;
        }
        self.nodes.push(LedgerNode {
            previous: self.tail,
            records,
            count,
            decisions,
        });
        self.tail = Some(self.nodes.len() - 1);
        Ok(())
    }

    fn append_retrievals(&mut self, entries: &'a [RetrievalEntry]) -> Result<(), Failure> {
        let next = fold_retrieval_entries(self.retrieval_boundary(), entries)?;
        let mut tick = self.tick;
        for entry in entries {
            self.validator
                .validate_retrieval_entry(entry)
                .and_then(|()| {
                    self.validator
                        .validate_retrieval_entry_identity_bounds(entry)
                })
                .map_err(|_| invalid_archive())?;
            if entry.tick < tick {
                return Err(invalid_archive());
            }
            // Only Created claims an allocation origin. The shared set also
            // includes ordinary floor items, actor stock and wizard births.
            if let RetrievalResult::Created { item, .. } = entry.result {
                if !self.item_origins.insert(item.0) {
                    return Err(invalid_archive());
                }
            }
            tick = entry.tick;
        }
        if let Some(next) = next {
            self.retrieval.item = next.item;
            self.retrieval.entries = next.entry_count;
            self.retrieval.placements = next.placement_count;
        }
        self.retrieval.latest = entries;
        Ok(())
    }

    fn retrieval_boundary(&self) -> Option<RetrievalBoundaryState> {
        self.validator
            .unique_retrieval_policy()
            .map(|_| RetrievalBoundaryState {
                initialized: true,
                item: self.retrieval.item,
                entry_count: self.retrieval.entries,
                placement_count: self.retrieval.placements,
            })
    }

    pub(super) fn observe(&mut self, record: &'a Record) -> Result<(), Failure> {
        let wizard = matches!(record.entry.content, JournalContent::Wizard { .. });
        if record.before_tick != self.tick
            || record.retrieval_before != self.retrieval_boundary()
            || record.entry.tick
                != if wizard {
                    record.after_tick
                } else {
                    record.before_tick
                }
            || self.tail.is_some_and(|tail| {
                self.nodes[tail]
                    .records
                    .last()
                    .expect("nonempty receipt slice")
                    .tick
                    > record.before_tick
            })
        {
            return Err(invalid_archive());
        }
        let unique_birth = match &record.entry.content {
            JournalContent::Wizard {
                result: WizardResult::ActorSpawned { actor },
                ..
            } => self.actor_origins.insert(actor.0),
            JournalContent::Wizard {
                result: WizardResult::ItemPlaced { item },
                ..
            } => self.item_origins.insert(*item),
            _ => true,
        };
        if !unique_birth {
            return Err(invalid_archive());
        }
        if let JournalContent::Wizard {
            operation: WizardOperation::Rewind { target },
            ..
        } = &record.entry.content
        {
            if !record.activations.is_empty() || !record.retrievals.is_empty() {
                return Err(invalid_archive());
            }
            let boundary = self
                .boundaries
                .iter()
                .find(|boundary| boundary.selectable && &boundary.id == target)
                .ok_or_else(invalid_archive)?;
            if record.after_tick != boundary.tick {
                return Err(invalid_archive());
            }
            self.tail = boundary.tail;
            self.retrieval = boundary.retrieval;
            self.retrieval.latest = &[];
            self.scopes.clear();
            let mut node = self.tail;
            while let Some(index) = node {
                self.scopes.extend(
                    self.nodes[index]
                        .records
                        .iter()
                        .map(|record| record.scope.as_str()),
                );
                node = self.nodes[index].previous;
            }
        } else {
            let may_activate = matches!(
                record.entry.content,
                JournalContent::Action { .. }
                    | JournalContent::IntentionStarted { .. }
                    | JournalContent::IntentionContinued { .. }
                    | JournalContent::IntentionChanged {
                        change: crate::journal::IntentionChange::Escaped { .. },
                        ..
                    }
                    | JournalContent::Wizard { .. }
            );
            if record.after_tick < record.before_tick
                || (!may_activate
                    && (!record.activations.is_empty() || !record.retrievals.is_empty()))
                || record.retrievals.iter().any(|entry| {
                    entry.cause == EntryCause::Initial
                        || entry.tick < record.before_tick
                        || entry.tick > record.after_tick
                })
                || record.activations.iter().any(|activation| {
                    activation.cause == EntryCause::Initial
                        || activation.tick < record.before_tick
                        || activation.tick > record.after_tick
                })
            {
                return Err(invalid_archive());
            }
            self.append(&record.activations)?;
            if !matches!(record.entry.content, JournalContent::Annotation { .. }) {
                self.append_retrievals(&record.retrievals)?;
            }
        }
        if matches!(record.entry.content, JournalContent::Annotation { .. })
            && record.after_tick != record.before_tick
        {
            return Err(invalid_archive());
        }
        if record.retrieval_after != self.retrieval_boundary() {
            return Err(invalid_archive());
        }
        self.tick = record.after_tick;
        if !matches!(record.entry.content, JournalContent::Annotation { .. }) {
            self.boundaries.push_back(LedgerBoundary {
                id: Some(record.entry.id.clone()),
                selectable: record.entry.content.rewindable(),
                tail: self.tail,
                tick: self.tick,
                retrieval: self.retrieval,
            });
            retain_boundaries(&mut self.boundaries, |boundary| boundary.selectable);
        }
        Ok(())
    }

    pub(super) fn valid_game(&self, game: &Game, boundary: &Option<EntryId>) -> bool {
        let Some(saved) = self.boundaries.iter().find(|saved| &saved.id == boundary) else {
            return false;
        };
        let valid_retrieval = match game.unique_retrieval_state() {
            Some(state) => {
                state.initialized
                    && state.item == saved.retrieval.item
                    && state.entry_count == saved.retrieval.entries
                    && state.placement_count == saved.retrieval.placements
                    && state.entries == saved.retrieval.latest
            }
            None => {
                saved.retrieval.item.is_none()
                    && saved.retrieval.entries == 0
                    && saved.retrieval.placements == 0
                    && saved.retrieval.latest.is_empty()
            }
        };
        if !valid_retrieval || game.tick() != saved.tick {
            return false;
        }
        let Ok(actual) = game.activations_since(0) else {
            return false;
        };
        if actual.len() != saved.tail.map_or(0, |tail| self.nodes[tail].count) {
            return false;
        }
        let mut actual = actual.iter().rev();
        let mut node = saved.tail;
        while let Some(index) = node {
            for expected in self.nodes[index].records.iter().rev() {
                if actual.next() != Some(expected) {
                    return false;
                }
            }
            node = self.nodes[index].previous;
        }
        actual.next().is_none()
    }
}
