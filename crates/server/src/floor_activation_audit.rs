//! Private checkpoint audit of exact entry receipts, including abandoned branches.
//! Journal replay compares each executed delta directly; checkpoint recovery must
//! prove the same ledger correspondence without replaying gameplay effects.
use super::*;
use tor_simulation::floor_activation::{
    EntryCause, PlacementDecision, StageResult, MAX_RETAINED_DECISIONS, MAX_SCOPES,
};

struct LedgerNode<'a> {
    previous: Option<usize>,
    records: &'a [FloorActivationRecord],
    count: usize,
    decisions: usize,
}

struct LedgerBoundary {
    id: Option<EntryId>,
    selectable: bool,
    tail: Option<usize>,
}

pub(super) struct ActivationAudit<'a> {
    validator: &'a Game,
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
    boundaries: VecDeque<LedgerBoundary>,
}

impl<'a> ActivationAudit<'a> {
    pub(super) fn new(
        initial: &'a [FloorActivationRecord],
        validator: &'a Game,
    ) -> Result<Self, Failure> {
        if initial.len() > 1
            || initial
                .iter()
                .any(|record| record.cause != EntryCause::Initial || record.tick != 0)
        {
            return Err(invalid_archive());
        }
        let mut audit = Self {
            validator,
            nodes: Vec::new(),
            tail: None,
            scopes: BTreeSet::new(),
            actor_origins: BTreeSet::new(),
            item_origins: BTreeSet::new(),
            boundaries: VecDeque::new(),
        };
        audit.append(initial)?;
        audit.boundaries.push_back(LedgerBoundary {
            id: None,
            selectable: true,
            tail: audit.tail,
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

    pub(super) fn observe(&mut self, record: &'a Record) -> Result<(), Failure> {
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
            if !record.activations.is_empty() {
                return Err(invalid_archive());
            }
            self.tail = self
                .boundaries
                .iter()
                .find(|boundary| boundary.selectable && &boundary.id == target)
                .ok_or_else(invalid_archive)?
                .tail;
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
            if (!may_activate && !record.activations.is_empty())
                || record.activations.iter().any(|activation| {
                    activation.cause == EntryCause::Initial
                        || (!matches!(record.entry.content, JournalContent::Wizard { .. })
                            && activation.tick < record.entry.tick)
                })
                || self.tail.is_some_and(|tail| {
                    self.nodes[tail]
                        .records
                        .last()
                        .expect("nonempty receipt slice")
                        .tick
                        > record.entry.tick
                })
            {
                return Err(invalid_archive());
            }
            self.append(&record.activations)?;
        }
        if !matches!(record.entry.content, JournalContent::Annotation { .. }) {
            self.boundaries.push_back(LedgerBoundary {
                id: Some(record.entry.id.clone()),
                selectable: record.entry.content.rewindable(),
                tail: self.tail,
            });
            retain_boundaries(&mut self.boundaries, |boundary| boundary.selectable);
        }
        Ok(())
    }

    pub(super) fn valid_game(&self, game: &Game, boundary: &Option<EntryId>) -> bool {
        let Some(saved) = self.boundaries.iter().find(|saved| &saved.id == boundary) else {
            return false;
        };
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
