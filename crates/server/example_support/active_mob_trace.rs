//! Offline workload semantic trace, shared with the existing server unit target.
use crate::host::journal::{Action, Event, JournalContent, JournalEntry};
use serde_json::json;
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Workload-scoped semantic oracle, used only by the offline save inspector.
/// Only these execution kinds are supported: extending the fixture requires an
/// explicit audit, not recursive deletion of arbitrary UUID-looking strings.
#[derive(Default)]
pub(super) struct ExecutionTrace(Sha256);
impl ExecutionTrace {
    pub(super) fn observe(&mut self, entry: &JournalEntry) -> Result<()> {
        let (action, event, admission) = match &entry.content {
            JournalContent::Action { action, event } => (action, event, None),
            JournalContent::IntentionStarted {
                action,
                event,
                admission,
                ..
            }
            | JournalContent::IntentionContinued {
                action,
                event,
                admission,
                ..
            } => (action, event, Some(admission)),
            _ => return Err("unsupported execution-trace journal kind".into()),
        };
        // These fields contain only authored content IDs, physical numeric IDs,
        // directions/positions and disclosure enums, never salted wire handles.
        require(
            matches!(
                (action, event),
                (Action::Move { .. }, Event::Moved { .. })
                    | (Action::Wait, Event::Waited)
                    | (Action::UseAbility { .. }, Event::AbilityStarted { .. })
            ),
            "unsupported execution-trace action/event",
        )?;
        let mut content = serde_json::to_value(&entry.content)?;
        let fields = content
            .as_object_mut()
            .ok_or("non-object journal content")?;
        let removed = fields.remove("admission");
        require(
            removed == admission.map(|id| json!(id.0)),
            "unexpected execution-trace admission schema",
        )?;
        // Numeric intention identities and the full action/event remain intact.
        // Entry/branch UUIDs are not selected; no other content is normalized.
        let mut semantic = json!({
            "actor":entry.actor.0,"tick":entry.tick,"content":content,
            "intention_suspensions":entry.intention_suspensions,
            "intention_ends":entry.intention_ends,
        });
        // Lexicographically sorted object keys, intact array order, compact JSON.
        // The u64 little-endian length prefix unambiguously separates executions.
        semantic.sort_all_objects();
        let bytes = serde_json::to_vec(&semantic)?;
        self.0.update(u64::try_from(bytes.len())?.to_le_bytes());
        self.0.update(bytes);
        Ok(())
    }
    pub(super) fn finish(self) -> String {
        self.0
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

/// Counts only committed Action/Started/Continued executions, not admissions.
/// Every execution shape is validated, including the tail beyond a selected
/// prefix; this fixture oracle never silently accepts unreviewed action types.
pub(super) struct InspectionTrace {
    limit: Option<usize>,
    available: usize,
    selected: usize,
    trace: ExecutionTrace,
}
#[derive(Debug)]
pub(super) struct TraceSummary {
    pub(super) count: usize,
    pub(super) available: usize,
    pub(super) sha256: String,
}
impl InspectionTrace {
    pub(super) fn new(limit: Option<usize>) -> Result<Self> {
        require(limit != Some(0), "trace executions must be positive")?;
        Ok(Self {
            limit,
            available: 0,
            selected: 0,
            trace: ExecutionTrace::default(),
        })
    }
    pub(super) fn observe(&mut self, entry: &JournalEntry) -> Result<()> {
        // This successful-workload oracle must not hide failed work among the
        // excluded admissions, including failures after a selected prefix.
        require(
            !matches!(
                &entry.content,
                JournalContent::IntentionFailed { .. }
                    | JournalContent::IntentionContinuationFailed { .. }
            ),
            "failed intention in successful-workload execution trace",
        )?;
        if !matches!(
            &entry.content,
            JournalContent::Action { .. }
                | JournalContent::IntentionStarted { .. }
                | JournalContent::IntentionContinued { .. }
        ) {
            return Ok(());
        }
        self.available = self
            .available
            .checked_add(1)
            .ok_or("execution count overflow")?;
        if self.limit.is_none_or(|limit| self.selected < limit) {
            self.trace.observe(entry)?;
            self.selected += 1;
        } else {
            // Audit even the unselected tail, but never add it to the prefix hash.
            ExecutionTrace::default().observe(entry)?;
        }
        Ok(())
    }
    pub(super) fn finish(self) -> Result<TraceSummary> {
        require(self.available > 0, "no committed executions in save")?;
        require(
            self.limit.is_none_or(|limit| limit <= self.available),
            "trace executions exceeds available committed executions",
        )?;
        Ok(TraceSummary {
            count: self.selected,
            available: self.available,
            sha256: self.trace.finish(),
        })
    }
}

// Workload-v1 bound, not a scheduler or AI pacing change: base/cardinal turns
// cost 100 ticks; diagonal movement ceil(sqrt(2) * 100) = 142; fixed melee 80 + 20 = 100.
// Requiring a meaningful action at most 200 ticks apart covers every two player
// turns, including the final partial AI round when a real client stops the save.
const PLAYER_TURN_TICKS: u64 = 100;
const MEANINGFUL_GAP_LIMIT: u64 = 200;
#[derive(Default)]
pub(super) struct SustainedActivity {
    player_moves: u64,
    player_last_tick: Option<u64>,
    actors: std::collections::BTreeMap<u64, MeaningfulActivity>,
}
#[derive(Debug, serde::Serialize)]
struct MeaningfulActivity {
    meaningful_actions: u64,
    first_tick: u64,
    last_tick: u64,
    max_gap_ticks: u64,
}
#[derive(Debug, serde::Serialize)]
pub(super) struct SustainedProof {
    player_moves: u64,
    player_turn_ticks: u64,
    player_first_tick: u64,
    player_last_tick: u64,
    max_gap_limit_ticks: u64,
    actors: std::collections::BTreeMap<u64, MeaningfulActivity>,
}
impl SustainedActivity {
    pub(super) fn observe(&mut self, entry: &JournalEntry) -> Result<()> {
        let event = match &entry.content {
            JournalContent::Action { event, .. }
            | JournalContent::IntentionStarted { event, .. }
            | JournalContent::IntentionContinued { event, .. } => event,
            _ => return Ok(()),
        };
        if entry.actor.0 == 1 {
            require(
                matches!(event, Event::Moved { .. }),
                "non-move player execution in sustained workload",
            )?;
            require(
                Some(entry.tick) == self.player_moves.checked_mul(PLAYER_TURN_TICKS),
                "player movement ticks are not the canonical workload sequence",
            )?;
            self.player_moves = self
                .player_moves
                .checked_add(1)
                .ok_or("player move count overflow")?;
            self.player_last_tick = Some(entry.tick);
            return Ok(());
        }
        if !matches!(event, Event::Moved { .. } | Event::AbilityStarted { .. }) {
            return Ok(());
        }
        require(
            (2..=101).contains(&entry.actor.0),
            "unexpected meaningful AI identity",
        )?;
        let row = self
            .actors
            .entry(entry.actor.0)
            .or_insert(MeaningfulActivity {
                meaningful_actions: 0,
                first_tick: entry.tick,
                last_tick: entry.tick,
                max_gap_ticks: 0,
            });
        let gap = entry
            .tick
            .checked_sub(row.last_tick)
            .ok_or("meaningful AI ticks went backwards")?;
        row.max_gap_ticks = row.max_gap_ticks.max(gap);
        row.meaningful_actions = row
            .meaningful_actions
            .checked_add(1)
            .ok_or("meaningful action count overflow")?;
        row.last_tick = entry.tick;
        Ok(())
    }
    pub(super) fn finish(self) -> Result<SustainedProof> {
        let last = self
            .player_last_tick
            .ok_or("no player movement in sustained workload")?;
        require(
            self.actors.len() == 100,
            "all 100 AI need sustained meaningful activity",
        )?;
        for (id, row) in &self.actors {
            if row.first_tick != 0 {
                return Err(format!("AI {id} did not start meaningful activity at tick 0").into());
            }
            if row.max_gap_ticks > MEANINGFUL_GAP_LIMIT {
                return Err(format!("AI {id} meaningful action gap exceeds 200 ticks").into());
            }
            if last.saturating_sub(row.last_tick) > MEANINGFUL_GAP_LIMIT {
                return Err(
                    format!("AI {id} stopped meaningful activity before the final window").into(),
                );
            }
        }
        Ok(SustainedProof {
            player_moves: self.player_moves,
            player_turn_ticks: PLAYER_TURN_TICKS,
            player_first_tick: 0,
            player_last_tick: last,
            max_gap_limit_ticks: MEANINGFUL_GAP_LIMIT,
            actors: self.actors,
        })
    }
}

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

#[cfg(test)]
mod execution_trace_tests {
    use super::*;
    use crate::host::journal::{
        Direction, IntentionEnd, IntentionEndKind, IntentionSuspension, Position,
    };
    use tor_protocol::{ActorId, Audience, Author, BranchId, EntryId};

    fn entry() -> JournalEntry {
        JournalEntry {
            id: EntryId("entry-a".into()),
            branch: BranchId("branch-a".into()),
            actor: ActorId(2),
            tick: 100,
            author: Author::Backend {
                component: "ai".into(),
            },
            audience: Audience::Actor,
            intention_suspensions: vec![],
            intention_ends: vec![],
            content: JournalContent::IntentionStarted {
                admission: EntryId("admission-a".into()),
                intention: tor_simulation::IntentionId(7),
                action: Action::Move {
                    direction: Direction::East,
                },
                event: Event::Moved {
                    from: Position {
                        region: 1,
                        x: 1,
                        y: 3,
                        z: 0,
                    },
                    to: Position {
                        region: 1,
                        x: 2,
                        y: 3,
                        z: 0,
                    },
                },
            },
        }
    }
    fn hash(entries: &[JournalEntry]) -> String {
        let mut trace = ExecutionTrace::default();
        for entry in entries {
            trace.observe(entry).unwrap();
        }
        trace.finish()
    }
    #[test]
    fn trace_uses_canonical_json_and_little_endian_length_framing() {
        // Independently authored canonical serialization of entry(), not another
        // invocation of the helper under test. Physical actor identity is numeric.
        let canonical = br#"{"actor":2,"content":{"action":{"direction":"east","type":"move"},"event":{"from":{"region":1,"x":1,"y":3,"z":0},"to":{"region":1,"x":2,"y":3,"z":0},"type":"moved"},"intention":7,"type":"intention_started"},"intention_ends":[],"intention_suspensions":[],"tick":100}"#;
        assert_eq!(canonical.len(), 265);
        assert_eq!(
            hash(&[entry()]),
            "9558a5a45a6c1f473be50d22ac03be6e5c31391a5a7496c62b398dff7fd04f6e"
        );
    }
    #[test]
    fn trace_ignores_only_selected_journal_uuids() {
        let first = entry();
        let mut second = first.clone();
        second.id = EntryId("entry-b".into());
        second.branch = BranchId("branch-b".into());
        let JournalContent::IntentionStarted { admission, .. } = &mut second.content else {
            unreachable!()
        };
        *admission = EntryId("admission-b".into());
        assert_eq!(hash(&[first]), hash(&[second]));
    }
    #[test]
    fn trace_retains_order_ticks_operands_and_numeric_lifecycle_links() {
        let baseline = entry();
        let expected = hash(std::slice::from_ref(&baseline));
        let mut mutations = vec![baseline.clone(); 7];
        mutations[0].actor = ActorId(3);
        mutations[1].tick += 1;
        let JournalContent::IntentionStarted { intention, .. } = &mut mutations[2].content else {
            unreachable!()
        };
        intention.0 += 1;
        let JournalContent::IntentionStarted { action, .. } = &mut mutations[3].content else {
            unreachable!()
        };
        *action = Action::Move {
            direction: Direction::West,
        };
        let JournalContent::IntentionStarted {
            event: Event::Moved { to, .. },
            ..
        } = &mut mutations[4].content
        else {
            unreachable!()
        };
        to.region = 2;
        mutations[5]
            .intention_suspensions
            .push(IntentionSuspension {
                actor: ActorId(3),
                intention: tor_simulation::IntentionId(4),
            });
        mutations[6].intention_ends.push(IntentionEnd {
            actor: ActorId(4),
            intention: tor_simulation::IntentionId(3),
            kind: IntentionEndKind::Resolved,
        });
        for mutation in mutations {
            assert_ne!(expected, hash(&[mutation]));
        }
        let mut later = baseline.clone();
        later.tick += 1;
        assert_ne!(
            hash(&[baseline.clone(), later.clone()]),
            hash(&[later, baseline])
        );
    }
    #[test]
    fn trace_rejects_unreviewed_content_and_action_event_shapes() {
        let mut unsupported = entry();
        unsupported.content = JournalContent::AutonomousIntentionAdmitted {
            intention: tor_simulation::IntentionId(7),
        };
        assert!(ExecutionTrace::default().observe(&unsupported).is_err());
        let mut inconsistent = entry();
        let JournalContent::IntentionStarted { event, .. } = &mut inconsistent.content else {
            unreachable!()
        };
        *event = Event::Waited;
        assert!(ExecutionTrace::default().observe(&inconsistent).is_err());
    }

    #[test]
    fn inspection_prefix_counts_executions_not_admissions_and_defaults_to_all() {
        let first = entry();
        let mut continued = first.clone();
        continued.tick += 1;
        let JournalContent::IntentionStarted {
            admission,
            intention,
            action,
            event,
        } = continued.content
        else {
            unreachable!()
        };
        continued.content = JournalContent::IntentionContinued {
            admission,
            intention,
            action,
            event,
        };
        let mut immediate = first.clone();
        immediate.tick += 2;
        immediate.content = JournalContent::Action {
            action: Action::Wait,
            event: Event::Waited,
        };
        let mut admission = first.clone();
        admission.content = JournalContent::AutonomousIntentionAdmitted {
            intention: tor_simulation::IntentionId(7),
        };
        let entries = [
            admission.clone(),
            first.clone(),
            admission,
            continued.clone(),
            immediate.clone(),
        ];
        for (limit, count, expected) in [
            (Some(1), 1, hash(std::slice::from_ref(&first))),
            (Some(2), 2, hash(&[first.clone(), continued.clone()])),
            (
                Some(3),
                3,
                hash(&[first.clone(), continued.clone(), immediate.clone()]),
            ),
            (
                None,
                3,
                hash(&[first.clone(), continued.clone(), immediate]),
            ),
        ] {
            let mut trace = InspectionTrace::new(limit).unwrap();
            for entry in &entries {
                trace.observe(entry).unwrap();
            }
            let summary = trace.finish().unwrap();
            assert_eq!(summary.count, count);
            assert_eq!(summary.available, 3);
            assert_eq!(summary.sha256, expected);
        }
    }
    #[test]
    fn inspection_rejects_empty_zero_oversized_and_unreviewed_tail() {
        assert!(InspectionTrace::new(Some(0)).is_err());
        assert!(InspectionTrace::new(None).unwrap().finish().is_err());
        let valid = entry();
        let mut trace = InspectionTrace::new(Some(2)).unwrap();
        trace.observe(&valid).unwrap();
        assert!(trace
            .finish()
            .unwrap_err()
            .to_string()
            .contains("exceeds available"));
        let mut trace = InspectionTrace::new(Some(1)).unwrap();
        trace.observe(&valid).unwrap();
        let mut invalid = valid;
        let JournalContent::IntentionStarted { event, .. } = &mut invalid.content else {
            unreachable!()
        };
        *event = Event::Waited;
        assert!(trace.observe(&invalid).is_err());
    }

    #[test]
    fn inspection_rejects_failed_intentions_inside_and_after_selected_prefix() {
        let valid = entry();
        let mut positive = InspectionTrace::new(Some(1)).unwrap();
        positive.observe(&valid).unwrap();
        let positive = positive.finish().unwrap();
        assert_eq!(positive.count, 1);
        assert_eq!(positive.sha256, hash(std::slice::from_ref(&valid)));
        for content in [
            JournalContent::IntentionFailed {
                admission: EntryId("failed-admission".into()),
                intention: tor_simulation::IntentionId(8),
            },
            JournalContent::IntentionContinuationFailed {
                admission: EntryId("failed-admission".into()),
                intention: tor_simulation::IntentionId(8),
            },
        ] {
            let mut failed = valid.clone();
            failed.content = content;
            for limit in [None, Some(1)] {
                for valid_first in [false, true] {
                    let mut trace = InspectionTrace::new(limit).unwrap();
                    if valid_first {
                        trace.observe(&valid).unwrap();
                    }
                    let error = trace.observe(&failed).unwrap_err();
                    assert!(error.to_string().contains("failed intention"));
                }
            }
        }
    }

    fn sustained_entries() -> Vec<JournalEntry> {
        let mut entries = Vec::new();
        for turn in 0..7 {
            for actor in 1..=101 {
                let mut value = entry();
                value.actor = ActorId(actor);
                value.tick = turn * 100;
                entries.push(value);
            }
        }
        entries
    }
    fn sustained(entries: &[JournalEntry]) -> Result<SustainedProof> {
        let mut proof = SustainedActivity::default();
        for entry in entries {
            proof.observe(entry)?;
        }
        proof.finish()
    }
    fn wait(entry: &mut JournalEntry) {
        entry.content = JournalContent::Action {
            action: Action::Wait,
            event: Event::Waited,
        };
    }
    #[test]
    fn sustained_activity_covers_all_ids_through_bounded_gaps() {
        let mut entries = sustained_entries();
        let full = sustained(&entries).unwrap();
        assert_eq!(full.player_moves, 7);
        assert_eq!(full.player_first_tick, 0);
        assert_eq!(full.player_last_tick, 600);
        assert_eq!(full.player_turn_ticks, 100);
        assert_eq!(full.max_gap_limit_ticks, 200);
        assert_eq!(
            full.actors.keys().copied().collect::<Vec<_>>(),
            (2..=101).collect::<Vec<_>>()
        );
        assert!(full
            .actors
            .values()
            .all(|row| row.meaningful_actions == 7 && row.max_gap_ticks == 100));
        for entry in &mut entries {
            if entry.actor == ActorId(101) && !entry.tick.is_multiple_of(200) {
                wait(entry);
            }
        }
        let bounded = sustained(&entries).unwrap();
        assert_eq!(bounded.actors[&101].max_gap_ticks, 200);
        assert_eq!(bounded.actors[&101].meaningful_actions, 4);
        // Final partial AI round: exactly200 ticks behind is still in the bound.
        for entry in &mut entries {
            if entry.actor == ActorId(101) && entry.tick == 600 {
                wait(entry);
            }
        }
        assert_eq!(sustained(&entries).unwrap().actors[&101].last_tick, 400);
    }
    #[test]
    fn sustained_activity_rejects_middle_stall_with_identical_endpoints_and_count() {
        let mut entries = sustained_entries();
        let full = sustained(&entries).unwrap();
        for entry in &mut entries {
            if entry.actor == ActorId(101) {
                entry.tick = match entry.tick {
                    100 => 20,
                    200 => 40,
                    tick => tick,
                };
            }
        }
        entries.sort_by_key(|entry| entry.tick);
        let ticks = entries
            .iter()
            .filter(|entry| entry.actor == ActorId(101))
            .map(|entry| entry.tick)
            .collect::<Vec<_>>();
        assert_eq!(ticks, [0, 20, 40, 300, 400, 500, 600]);
        assert_eq!(ticks.len() as u64, full.actors[&101].meaningful_actions);
        assert_eq!(ticks[0], full.actors[&101].first_tick);
        assert_eq!(*ticks.last().unwrap(), full.actors[&101].last_tick);
        assert!(sustained(&entries)
            .unwrap_err()
            .to_string()
            .contains("AI 101 meaningful action gap"));
    }
    #[test]
    fn sustained_activity_rejects_late_waits_missing_ids_and_noncanonical_player_ticks() {
        let valid = sustained_entries();
        sustained(&valid).unwrap();
        let mut late_waits = valid.clone();
        for entry in &mut late_waits {
            if entry.actor == ActorId(101) && entry.tick > 100 {
                wait(entry);
            }
        }
        assert_eq!(late_waits.len(), valid.len());
        assert!(sustained(&late_waits)
            .unwrap_err()
            .to_string()
            .contains("AI 101 stopped meaningful activity"));
        let missing = valid
            .iter()
            .filter(|entry| entry.actor != ActorId(101))
            .cloned()
            .collect::<Vec<_>>();
        assert!(sustained(&missing)
            .unwrap_err()
            .to_string()
            .contains("all 100 AI"));
        let mut late_start = valid.clone();
        wait(
            late_start
                .iter_mut()
                .find(|entry| entry.actor == ActorId(101))
                .unwrap(),
        );
        assert!(sustained(&late_start)
            .unwrap_err()
            .to_string()
            .contains("AI 101 did not start"));
        let mut bad_player = valid;
        bad_player
            .iter_mut()
            .find(|entry| entry.actor == ActorId(1) && entry.tick == 200)
            .unwrap()
            .tick = 201;
        assert!(sustained(&bad_player)
            .unwrap_err()
            .to_string()
            .contains("canonical workload sequence"));
    }
}
