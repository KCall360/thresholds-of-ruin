#[cfg(test)]
use crate::journal::Direction;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tor_protocol::*;
use tor_simulation::{ActorId as SimActor, Game};
use uuid::Uuid;

use crate::adapt;
use crate::journal::{
    Action, Command, JournalContent, JournalEntry, Position, WizardItem, WizardOperation,
    WizardResult,
};

pub(crate) const ARCHIVE_VERSION: u32 = 22;
#[path = "checkpoint.rs"]
mod checkpoint;
#[path = "command_request.rs"]
mod command_request;
#[path = "intention.rs"]
mod intention;
#[path = "intention_lifecycle.rs"]
mod intention_lifecycle;
#[cfg(test)]
#[path = "intention_tests.rs"]
mod intention_tests;
#[path = "wire_request.rs"]
mod wire_request;
pub(crate) use checkpoint::{Checkpoint, DiskCheckpoint};
const REWIND_BOUNDARIES: usize = 128;
const MAX_RETAINED_BOUNDARIES: usize = REWIND_BOUNDARIES * 2;
const RULESET: &str = crate::scenario_package::RULESET;

#[cfg(test)]
mod request_validation_tests {
    use super::*;

    #[test]
    fn receipts_and_authority_keep_their_precedence_over_stale_revisions() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let receipt = Receipt {
            user: "test".into(),
            frontend: "test".into(),
            request_id: "accepted".into(),
            actor,
            branch: engine.branch().clone(),
            command: Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: Action::Wait,
            },
        };
        let accepted = engine.apply_command(&receipt, None).unwrap();
        let state = engine.state(actor).unwrap();
        let mut profile = CommandProfile::default();
        let repeated = engine
            .apply_command_profiled(&receipt, None, Some(&mut profile))
            .unwrap();
        assert!(repeated.duplicate);
        assert_eq!(repeated.entry, accepted.entry);
        assert_eq!(profile.candidate_captures, 0);

        let mut conflict = receipt.clone();
        conflict.command = Command::Act {
            expected_revision: 0,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        let mut unauthorized = receipt.clone();
        unauthorized.request_id = "wizard".into();
        unauthorized.branch = BranchId("unavailable".into());
        unauthorized.command = Command::Wizard {
            expected_revision: 0,
            operation: WizardOperation::Teleport {
                actor,
                position: Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                },
            },
        };
        let mut malformed = receipt.clone();
        malformed.user.clear();
        let mut wrong_branch = receipt.clone();
        wrong_branch.request_id = "wrong-branch".into();
        wrong_branch.branch = BranchId("unavailable".into());
        for (receipt, code) in [
            (conflict, ErrorCode::RequestConflict),
            (unauthorized, ErrorCode::Unauthorized),
            (malformed, ErrorCode::InvalidRequest),
            (wrong_branch, ErrorCode::WrongBranch),
        ] {
            let failure = engine
                .apply_command_profiled(&receipt, None, Some(&mut profile))
                .unwrap_err();
            assert_eq!(failure.code, code);
            assert_eq!(profile.candidate_captures, 0);
            assert_eq!(engine.state(actor).unwrap(), state);
        }
    }

    #[test]
    fn stale_commands_are_rejected_before_candidate_capture() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        let actor = ActorId(1);
        let before = engine.state(actor).unwrap();
        let expected_revision = before.revision + 1;
        let commands = [
            Command::Act {
                expected_revision,
                action: Action::Wait,
            },
            Command::Travel {
                expected_revision,
                destination: "unavailable".into(),
            },
            Command::RenamePlace {
                expected_revision,
                key: "unavailable".into(),
                name: "name".into(),
            },
            Command::Wizard {
                expected_revision,
                operation: WizardOperation::Teleport {
                    actor,
                    position: Position {
                        region: 1,
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                },
            },
        ];
        for (n, command) in commands.into_iter().enumerate() {
            let receipt = Receipt {
                user: "test".into(),
                frontend: "test".into(),
                request_id: format!("stale-{n}"),
                actor,
                branch: engine.branch().clone(),
                command,
            };
            let mut profile = CommandProfile::default();
            let failure = engine
                .apply_command_profiled(&receipt, None, Some(&mut profile))
                .unwrap_err();
            assert_eq!(failure.code, ErrorCode::StaleRevision);
            assert_eq!(
                profile.candidate_captures, 0,
                "stale command {n} copied candidate state"
            );
            assert_eq!(engine.state(actor).unwrap(), before);
        }
    }
}

#[cfg(test)]
mod observation_reuse_tests {
    use super::*;

    fn assert_matches_uncached(engine: &Engine, actor: ActorId) {
        assert_eq!(
            engine.state(actor).unwrap(),
            engine.diagnostic_copy().state(actor).unwrap()
        );
        let before = tor_simulation::diagnostics::work_counts();
        engine.state(actor).unwrap();
        assert_eq!(
            tor_simulation::diagnostics::work_counts().observations,
            before.observations
        );
    }

    #[test]
    fn wizard_mutation_rewind_notes_and_seeded_fixtures_never_retain_stale_views() {
        let actor = ActorId(1);
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        engine.enable_wizard().unwrap();
        assert_matches_uncached(&engine, actor);
        let branch = engine.branch().clone();
        engine
            .command(
                "test",
                "test",
                actor,
                "teleport",
                &branch,
                Command::Wizard {
                    expected_revision: engine.revision(actor).unwrap(),
                    operation: WizardOperation::Teleport {
                        actor,
                        position: Position {
                            region: 2,
                            x: 1,
                            y: 1,
                            z: 0,
                        },
                    },
                },
            )
            .unwrap();
        assert_matches_uncached(&engine, actor);
        engine
            .command(
                "test",
                "test",
                actor,
                "rewind",
                &branch,
                Command::Wizard {
                    expected_revision: engine.revision(actor).unwrap(),
                    operation: WizardOperation::Rewind { target: None },
                },
            )
            .unwrap();
        assert_matches_uncached(&engine, actor);
        let state = engine.state(actor).unwrap();
        engine
            .command(
                "test",
                "test",
                actor,
                "note",
                &engine.branch().clone(),
                Command::Annotate {
                    anchor: Anchor::State {
                        revision: state.revision,
                    },
                    text: "private note".into(),
                    source: ClientSource::User,
                    audience: Audience::Private,
                    category: AnnotationCategory::Note,
                },
            )
            .unwrap();
        assert_eq!(engine.state(actor).unwrap(), state);
        assert_matches_uncached(&engine, actor);
        engine.seed_profile_history(3).unwrap();
        assert_matches_uncached(&engine, actor);
    }

    #[test]
    fn repeated_reads_reuse_the_boundary_and_committed_actions_replace_it() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let initial = engine.state(actor).unwrap();
        let before = tor_simulation::diagnostics::work_counts();
        assert_eq!(engine.state(actor).unwrap(), initial);
        assert_eq!(
            tor_simulation::diagnostics::work_counts().observations,
            before.observations
        );
        engine
            .command(
                "test",
                "test",
                actor,
                "move",
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: initial.revision,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        let before = tor_simulation::diagnostics::work_counts();
        let moved = engine.state(actor).unwrap();
        assert_ne!(moved, initial);
        assert_eq!(engine.state(actor).unwrap(), moved);
        assert_eq!(
            tor_simulation::diagnostics::work_counts().observations,
            before.observations,
            "reuse the post-action view used for revision comparison"
        );
        engine
            .command(
                "test",
                "test",
                actor,
                "wait",
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: moved.revision,
                    action: Action::Wait,
                },
            )
            .unwrap();
        let waited = engine.state(actor).unwrap();
        assert!(waited.observation.tick > moved.observation.tick);
        assert_eq!(engine.state(actor).unwrap(), waited);
    }
}

#[cfg(test)]
mod seed_equivalence_tests {
    use super::*;
    #[test]
    fn seeded_waits_preserve_game_navigation_scheduler_and_every_retained_boundary() {
        for actors in [1, 8] {
            let mut seeded = Engine::memory(Scenario::performance(42, 8, actors).unwrap()).unwrap();
            let mut normal = seeded.diagnostic_copy();
            seeded.seed_profile_history(3).unwrap();
            seeded.seed_profile_history(129).unwrap();
            for index in 0..132 {
                let actor = ActorId(normal.game.next_actor().unwrap().0);
                normal
                    .command(
                        "bench",
                        "headless",
                        actor,
                        &format!("seed-{index}"),
                        &normal.branch().clone(),
                        Command::Act {
                            expected_revision: normal.revision(actor).unwrap(),
                            action: Action::Wait,
                        },
                    )
                    .unwrap();
            }
            assert_eq!(seeded.game, normal.game);
            assert_eq!(seeded.revisions, normal.revisions);
            assert_eq!(seeded.boundaries.len(), normal.boundaries.len());
            for (a, b) in seeded.boundaries.iter().zip(&normal.boundaries) {
                assert_eq!(a.game, b.game);
                assert_eq!(a.revisions, b.revisions);
            }
            for (a, b) in seeded.archive.records.iter().zip(&normal.archive.records) {
                assert_eq!(a.receipt, b.receipt);
                assert_eq!(a.entry.content, b.entry.content);
                assert_eq!(a.entry.tick, b.entry.tick);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

impl Failure {
    pub(crate) fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorSetup {
    pub position: Position,
    pub turn_ticks: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub seed: u64,
    pub actors: Vec<ActorSetup>,
    pub regions: u64,
    pub workload_version: Option<u32>,
    pub package: Option<Arc<crate::scenario_package::Package>>,
    /// Region streaming for package games; `None` keeps every region
    /// active, as diagnostic fixtures do.
    pub streaming: Option<crate::regions::Streaming>,
}

impl Scenario {
    pub fn two_room(seed: u64) -> Self {
        Self {
            seed,
            actors: vec![ActorSetup {
                position: Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                },
                turn_ticks: 100,
            }],
            regions: 2,
            workload_version: None,
            package: None,
            streaming: None,
        }
    }

    pub fn performance(seed: u64, regions: u64, actors: usize) -> Result<Self, Failure> {
        if !(1..=256).contains(&regions) || !(1..=8).contains(&actors) {
            return Err(invalid_archive());
        }
        let fixture = crate::performance_fixture::specification();
        Ok(Self {
            seed,
            regions,
            workload_version: Some(fixture.version),
            package: None,
            streaming: None,
            actors: fixture
                .geometry
                .actors
                .into_iter()
                .take(actors)
                .map(|[x, y, z]| ActorSetup {
                    position: Position { region: 1, x, y, z },
                    turn_ticks: 100,
                })
                .collect(),
        })
    }
}

fn scenario_game(scenario: &Scenario) -> Game {
    if scenario.regions <= 2 {
        return Game::two_room_in_stone(scenario.seed);
    }
    let rooms = (1..=scenario.regions)
        .map(|id| tor_world::Region {
            id: tor_world::RegionId(id),
            name: format!("Region {id}"),
            bounds: tor_world::Extent::new(17, 17, 2).expect("valid benchmark extent"),
        })
        .collect();
    let mut world = tor_world::World::new(rooms, vec![]).expect("valid benchmark world");
    for id in 1..scenario.regions {
        for z in 0..2 {
            let from = tor_world::Location {
                region: tor_world::RegionId(id),
                position: tor_world::Position { x: 16, y: 8, z },
            };
            let to = tor_world::Location {
                region: tor_world::RegionId(id + 1),
                position: tor_world::Position { x: 0, y: 8, z },
            };
            world
                .connect(
                    tor_world::Passage {
                        from,
                        direction: tor_world::Direction::East,
                        to,
                    },
                    0,
                )
                .expect("valid benchmark passage");
            world
                .connect(
                    tor_world::Passage {
                        from: to,
                        direction: tor_world::Direction::West,
                        to: from,
                    },
                    0,
                )
                .expect("valid benchmark passage");
        }
    }
    for id in 1..=scenario.regions {
        for z in 0..2 {
            for (x, y) in [(4, 4), (4, 12), (12, 4), (12, 12)] {
                world
                    .set_wall(
                        tor_world::Location {
                            region: tor_world::RegionId(id),
                            position: tor_world::Position { x, y, z },
                        },
                        true,
                    )
                    .expect("valid benchmark wall");
            }
        }
    }
    Game::new(world, scenario.seed)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    user: String,
    frontend: String,
    request_id: String,
    #[serde(with = "crate::storage::schema::ActorId")]
    actor: ActorId,
    #[serde(with = "crate::storage::schema::BranchId")]
    branch: BranchId,
    command: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    pub(crate) entry: JournalEntry,
    pub(crate) receipt: Option<Receipt>,
}

impl Record {
    pub(crate) fn change_source(&self) -> Option<(&EntryId, tor_simulation::IntentionId)> {
        if let JournalContent::IntentionChanged {
            admission,
            intention,
            ..
        } = &self.entry.content
        {
            Some((admission, *intention))
        } else {
            None
        }
    }

    pub(crate) fn valid_change(&self, admitted: &Record) -> bool {
        use crate::journal::IntentionChange;
        let JournalContent::IntentionChanged {
            admission,
            intention,
            change,
        } = &self.entry.content
        else {
            return false;
        };
        let native_travel = matches!(admitted.entry.content,
            JournalContent::TravelIntentionAdmitted { intention: id, .. } if id == *intention);
        if admission != &admitted.entry.id
            || self.entry.actor != admitted.entry.actor
            || !(native_travel
                || matches!(admitted.entry.content, JournalContent::IntentionAdmitted { intention: id, .. } if id == *intention))
        {
            return false;
        }
        match &self.receipt {
            None => {
                *change
                    == if native_travel {
                        IntentionChange::Cancelled
                    } else {
                        IntentionChange::Suspended
                    }
                    && self.entry.audience == Audience::Actor
                    && self.entry.author
                        == Author::Backend {
                            component: "scheduler".into(),
                        }
            }
            Some(receipt) => {
                !native_travel
                    && receipt.actor == self.entry.actor
                    && receipt.branch == self.entry.branch
                    && self.entry.author
                        == Author::User {
                            user: receipt.user.clone(),
                        }
                    && self.entry.audience == Audience::Private
                    && match (&receipt.command, change) {
                        (
                            Command::ResumeIntention {
                                admission: requested,
                                ..
                            },
                            IntentionChange::Resumed,
                        )
                        | (
                            Command::CancelIntention {
                                admission: requested,
                                ..
                            },
                            IntentionChange::Cancelled,
                        ) => requested == admission,
                        _ => false,
                    }
            }
        }
    }
    pub(crate) fn resolution(&self) -> Option<(&EntryId, tor_simulation::IntentionId)> {
        match &self.entry.content {
            JournalContent::IntentionStarted {
                admission,
                intention,
                ..
            }
            | JournalContent::IntentionFailed {
                admission,
                intention,
            }
            | JournalContent::IntentionContinued {
                admission,
                intention,
                ..
            }
            | JournalContent::IntentionContinuationFailed {
                admission,
                intention,
            } => Some((admission, *intention)),
            _ => None,
        }
    }

    pub(crate) fn valid_resolution(&self, admitted: &Record) -> bool {
        let Some((admission, intention)) = self.resolution() else {
            return false;
        };
        let Some((original, work)) = admitted.entry.content.admission() else {
            return false;
        };
        let original_action = match work {
            crate::journal::AdmittedWork::Human(action)
            | crate::journal::AdmittedWork::Travel(action, _) => Some(action),
            crate::journal::AdmittedWork::AutonomousDecision => None,
        };
        admission == &admitted.entry.id
            && intention == original
            && self.entry.actor == admitted.entry.actor
            && self.receipt.is_none()
            && self.entry.author
                == Author::Backend {
                    component: "scheduler".into(),
                }
            && self.entry.audience == Audience::Actor
            && match &self.entry.content {
                JournalContent::IntentionStarted {
                    action: executed, ..
                } => original_action.is_none_or(|action| executed == action),
                JournalContent::IntentionContinued {
                    action: executed, ..
                } => original_action.is_some_and(|action| {
                    executed == action && matches!(action, Action::Attack { .. })
                }),
                JournalContent::IntentionFailed { .. } => true,
                JournalContent::IntentionContinuationFailed { .. } => {
                    original_action.is_some_and(|action| matches!(action, Action::Attack { .. }))
                }
                _ => false,
            }
    }

    /// Check audit metadata even when recovery restores a checkpoint instead of replaying.
    pub(crate) fn valid_admission(&self) -> bool {
        if self.receipt.as_ref().is_some_and(|receipt| {
            matches!(
                receipt.command,
                Command::ResumeIntention { .. } | Command::CancelIntention { .. }
            )
        }) && self.change_source().is_none()
        {
            return false;
        }
        match (&self.entry.content, self.receipt.as_ref()) {
            (JournalContent::IntentionAdmitted { intention, action }, Some(receipt)) => {
                intention.0 != 0
                    && matches!(&receipt.command, Command::AdmitIntention {
                        action: requested, ..
                    } if requested == action)
                    && receipt.actor == self.entry.actor
                    && receipt.branch == self.entry.branch
                    && self.entry.audience == Audience::Private
                    && self.entry.author
                        == Author::User {
                            user: receipt.user.clone(),
                        }
            }
            (
                JournalContent::TravelIntentionAdmitted {
                    intention,
                    journey,
                    step,
                    action,
                    destination,
                },
                None,
            ) => {
                intention.0 != 0
                    && *step != 0
                    && uuid::Uuid::parse_str(&journey.0).is_ok()
                    && destination.region.0 != 0
                    && matches!(action, Action::Move { .. })
                    && self.entry.audience == Audience::Private
                    && self.entry.author
                        == Author::Backend {
                            component: "scheduler".into(),
                        }
            }
            (JournalContent::TravelIntentionAdmitted { .. }, Some(_)) => false,
            (JournalContent::AutonomousIntentionAdmitted { intention }, None) => {
                intention.0 != 0
                    && self.entry.audience == Audience::Private
                    && self.entry.author
                        == Author::Backend {
                            component: "scheduler".into(),
                        }
            }
            (JournalContent::AutonomousIntentionAdmitted { .. }, Some(_)) => false,
            (JournalContent::IntentionAdmitted { .. }, None) => false,
            (_, Some(receipt)) => !matches!(receipt.command, Command::AdmitIntention { .. }),
            (_, None) => true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Archive {
    pub(crate) view_salt: String,
    pub(crate) wizard_game: bool,
    pub(crate) version: u32,
    pub(crate) ruleset: String,
    #[serde(with = "crate::storage::schema::scenario")]
    pub(crate) scenario: Scenario,
    #[serde(with = "crate::storage::schema::BranchId")]
    pub(crate) branch: BranchId,
    pub(crate) records: Vec<Record>,
}

/// How many of a streaming game's regions are in each state, and how its
/// region records are kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RegionCounts {
    pub active: usize,
    pub frozen: usize,
    pub detached: usize,
    pub unbuilt: usize,
    /// Records held in memory.
    pub resident_records: usize,
    /// Records read back from disk.
    pub records_read: usize,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub entry: JournalEntry,
    pub duplicate: bool,
}

/// Diagnostic phase measurements for the checked-in performance harness.
/// Durations are deliberately not used as correctness thresholds; the counts
/// and byte totals provide stable regression contracts.
#[derive(Clone, Debug, Default, Serialize)]
pub struct CommandProfile {
    /// Includes nested door validation and navigation observations.
    pub perception_calls: usize,
    pub scene_calls: usize,
    pub authoritative_total: Duration,
    pub rollback_capture: Duration,
    pub checkpoint_capture: Duration,
    pub checkpoint_captures: usize,
    pub simulation_transition: Duration,
    pub navigation_refresh: Duration,
    pub perception: Duration,
    pub revision_detection: Duration,
    pub rollback_snapshot: Duration,
    pub journal_serialization: Duration,
    pub journal_write: Duration,
    pub journal_sync: Duration,
    pub journal_replace: Duration,
    pub publication: Duration,
    pub simulation_transitions: usize,
    pub navigation_refreshes: usize,
    pub candidate_captures: usize,
    pub actors_observed: usize,
    pub revision_comparisons: usize,
    pub rollback_snapshots: usize,
    pub records_serialized: usize,
    pub bytes_written: u64,
    pub file_writes: usize,
    pub file_flushes: usize,
    pub file_syncs: usize,
    pub file_replacements: usize,
    /// Region streaming: the transition after the command, and its work.
    pub region_transition: Duration,
    /// Transitions that changed which regions are active or loaded.
    pub region_changes: usize,
    pub horizon_regions_expanded: usize,
    pub horizon_links_examined: usize,
    pub pinned_actors: usize,
    pub reach_lookups: usize,
    pub region_records_read: usize,
    pub regions_built: usize,
    /// Builds and reads the preloader had ready. Depends on timing, unlike
    /// the other counts.
    pub regions_prepared: usize,
    /// Nested acquisition details; excluded from `exclusive_duration` because
    /// their time is already included in `region_transition`.
    pub region_acquisition: crate::regions::RegionAcquisitionProfile,
    /// Background preloading: what it was asked for after the command, and
    /// the deterministic work of choosing it.
    pub preload_jobs: usize,
    pub preload_regions_expanded: usize,
    pub preload_links_examined: usize,
}

/// Startup measurements; retained history is read but only the checkpoint tail is simulated.
#[derive(Clone, Debug, Default, Serialize)]
pub struct RecoveryProfile {
    pub total: Duration,
    pub records_loaded: usize,
    pub records_replayed: usize,
    pub checkpoint_sequence: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BootstrapProfile {
    pub total: Duration,
    pub records_serialized: usize,
}

impl CommandProfile {
    /// Exclusive top-level phases. Nested simulation perception belongs to the
    /// simulation phase; navigation perception belongs to navigation.
    pub fn exclusive_duration(&self) -> Duration {
        self.rollback_capture
            + self.checkpoint_capture
            + self.simulation_transition
            + self.navigation_refresh
            + self.perception
            + self.revision_detection
            + self.rollback_snapshot
            + self.journal_serialization
            + self.journal_write
            + self.journal_sync
            + self.journal_replace
            + self.publication
            + self.region_transition
    }
}

/// Actors' revisions. Loaded actors' are the map itself; the rest are parked
/// in a shared map, so what every command copies stays bounded by what's
/// loaded. An actor moves between the two when its region attaches or
/// detaches, and its revision moves on each time.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Revisions {
    #[serde(with = "crate::storage::schema::revisions")]
    loaded: BTreeMap<ActorId, u64>,
    #[serde(with = "crate::storage::schema::shared_revisions")]
    parked: tor_world::Shared<BTreeMap<ActorId, u64>>,
}

impl std::ops::Deref for Revisions {
    type Target = BTreeMap<ActorId, u64>;
    fn deref(&self) -> &Self::Target {
        &self.loaded
    }
}

impl std::ops::DerefMut for Revisions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.loaded
    }
}

impl Revisions {
    fn new(game: &Game) -> Self {
        Self {
            loaded: game
                .loaded_actor_ids()
                .map(|id| (ActorId(id.0), 0))
                .collect(),
            parked: Default::default(),
        }
    }

    /// Any known actor's revision, loaded or not. An actor is parked once it
    /// leaves the loaded world; one never loaded is at revision zero.
    fn any(&self, actor: ActorId, game: &Game) -> Option<u64> {
        self.loaded
            .get(&actor)
            .or_else(|| self.parked.get(&actor))
            .copied()
            .or_else(|| game.known_actor_region(SimActor(actor.0)).map(|_| 0))
    }

    /// Follow actors into and out of loaded regions after a transition.
    fn follow(&mut self, game: &Game) -> Result<(), Failure> {
        let exhausted = || Failure::new(ErrorCode::InvalidAction, "Revision exhausted");
        let left: Vec<_> = self
            .loaded
            .keys()
            .copied()
            .filter(|id| !game.has_actor(SimActor(id.0)))
            .collect();
        for id in left {
            let revision = self.loaded.remove(&id).expect("listed actor");
            self.parked
                .insert(id, revision.checked_add(1).ok_or_else(exhausted)?);
        }
        for id in game.loaded_actor_ids().map(|id| ActorId(id.0)) {
            if !self.loaded.contains_key(&id) {
                let revision = self.parked.remove(&id).unwrap_or(0);
                self.loaded
                    .insert(id, revision.checked_add(1).ok_or_else(exhausted)?);
            }
        }
        Ok(())
    }

    /// Whether these revisions cover exactly `game`'s actors, split by
    /// whether they're loaded.
    pub(crate) fn valid_for(&self, game: &Game) -> bool {
        self.loaded
            .keys()
            .map(|a| a.0)
            .eq(game.loaded_actor_ids().map(|a| a.0))
            && self.parked.keys().all(|a| {
                let id = SimActor(a.0);
                !game.has_actor(id) && game.known_actor_region(id).is_some()
            })
    }
}

#[derive(Clone, Debug)]
struct Boundary {
    id: Option<EntryId>,
    /// Derived from journal content, never trusted from a checkpoint payload.
    selectable: bool,
    game: Game,
    revisions: Revisions,
}

/// Retain the union of the latest selectable states and latest raw transactions.
/// Queue lifecycle traffic cannot evict gameplay history; the raw window keeps
/// recent adjacent states available for checkpoint integrity checks.
fn retain_boundaries<T>(boundaries: &mut VecDeque<T>, selectable: impl Fn(&T) -> bool) {
    let first_raw = boundaries.len().saturating_sub(REWIND_BOUNDARIES);
    let mut old_selectable = boundaries
        .iter()
        .filter(|b| selectable(b))
        .count()
        .saturating_sub(REWIND_BOUNDARIES);
    let mut index = 0;
    boundaries.retain(|boundary| {
        let keep_selectable = if selectable(boundary) {
            if old_selectable == 0 {
                true
            } else {
                old_selectable -= 1;
                false
            }
        } else {
            false
        };
        let keep = index >= first_raw || keep_selectable;
        index += 1;
        keep
    });
}

/// Private mutable decision state. A transaction never owns retained history,
/// the receipt index, or a second storage handle.
#[derive(Clone, Debug)]
struct Candidate {
    current_branch: BranchId,
    boundaries: VecDeque<Arc<Boundary>>,
    game: Game,
    revisions: Revisions,
    observations: BTreeMap<ActorId, Arc<RevisionView>>,
    /// Region records this command's transition made.
    made: Vec<(
        tor_simulation::RecordId,
        tor_world::Shared<tor_simulation::RegionRecord>,
    )>,
}

impl Candidate {
    fn capture(engine: &Engine) -> Self {
        Self {
            current_branch: engine.current_branch.clone(),
            boundaries: engine.boundaries.clone(),
            game: engine.game.clone(),
            revisions: engine.revisions.clone(),
            observations: BTreeMap::new(),
            made: Vec::new(),
        }
    }
    fn branch(&self) -> &BranchId {
        &self.current_branch
    }
    /// Actors in loaded regions.
    fn actors(&self) -> Vec<ActorId> {
        self.revisions.keys().copied().collect()
    }
    fn revision_view(&self, actor: ActorId) -> Result<RevisionView, Failure> {
        revision_view(&self.game, actor)
    }
    /// Publish, then ask the preloader for what the next transitions are
    /// likely to need.
    fn publish(self, engine: &mut Engine) -> Option<crate::regions::PreloadWork> {
        engine.current_branch = self.current_branch;
        engine.boundaries = self.boundaries;
        engine.game = self.game;
        engine.revisions = self.revisions;
        engine.observations.replace(self.observations);
        let regions = engine.regions.as_mut()?;
        regions.publish(self.made);
        regions.preload(&engine.game)
    }

    /// Move to the regions the reference points ask for. When anything
    /// changed, every revision moves on, so clients refresh.
    fn transition(
        &mut self,
        regions: Option<&mut crate::regions::Regions>,
        profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        self.transition_with(regions, std::collections::BTreeSet::new(), profile)
    }

    /// Load and activate the regions a wizard operation acts on first, so it
    /// can reach places nobody has needed yet.
    fn load_for_wizard(
        &mut self,
        regions: Option<&mut crate::regions::Regions>,
        operation: &WizardOperation,
    ) -> Result<(), Failure> {
        let at = |p: &Position| tor_world::RegionId(p.region);
        let actor = |id: &ActorId| self.game.known_actor_region(SimActor(id.0));
        let targets: std::collections::BTreeSet<_> = match operation {
            WizardOperation::SetGravity { region, .. } => vec![Some(tor_world::RegionId(*region))],
            WizardOperation::SetCellGravity { position, .. }
            | WizardOperation::PlaceDoor { position, .. }
            | WizardOperation::SetPlaceHint { position, .. }
            | WizardOperation::SetWall { position, .. }
            | WizardOperation::PlaceItem { position, .. }
            | WizardOperation::SpawnActor { position, .. } => vec![Some(at(position))],
            WizardOperation::ConnectPortal { from, to, .. }
            | WizardOperation::ConnectArea { from, to, .. }
            | WizardOperation::Connect { from, to, .. } => vec![Some(at(from)), Some(at(to))],
            WizardOperation::Teleport {
                actor: id,
                position,
            } => vec![actor(id), Some(at(position))],
            WizardOperation::SetBody { actor: id, .. }
            | WizardOperation::SetVelocity { actor: id, .. }
            | WizardOperation::IdentifyItem { actor: id, .. } => vec![actor(id)],
            WizardOperation::PlaceChamber { .. }
            | WizardOperation::PlaceRoom { .. }
            | WizardOperation::Rewind { .. } => vec![],
        }
        .into_iter()
        .flatten()
        .collect();
        if targets.is_empty() {
            return Ok(());
        }
        self.transition_with(regions, targets, None)
    }

    fn transition_with(
        &mut self,
        regions: Option<&mut crate::regions::Regions>,
        extra: std::collections::BTreeSet<tor_world::RegionId>,
        profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        let Some(regions) = regions else {
            return Ok(());
        };
        let started = Instant::now();
        let work = regions.transition_with_profile(
            &mut self.game,
            &mut self.made,
            extra,
            profile.is_some(),
        )?;
        let report = &work.report;
        if let Some(profile) = profile {
            profile.region_transition += started.elapsed();
            profile.region_changes += usize::from(!report.is_empty());
            profile.horizon_regions_expanded += work.horizon_regions_expanded;
            profile.horizon_links_examined += work.horizon_links_examined;
            profile.pinned_actors += work.pinned_actors;
            profile.reach_lookups += work.reach_lookups;
            profile.region_records_read += work.records_read;
            profile.regions_built += report.built.len();
            profile.regions_prepared += work.prepared;
            profile.region_acquisition.add(work.acquisition);
        }
        if let Some(before) = work.before {
            // Loading, detaching, or activating regions can change both the
            // projected scene and scheduler readiness. Recompute at the new
            // boundary instead of publishing a pre-transition observation.
            self.observations.clear();
            // Only observers whose view the transition changed move on, so
            // an update never discloses a change nobody could see.
            let stayed: Vec<_> = self
                .revisions
                .keys()
                .copied()
                .filter(|id| self.game.has_actor(SimActor(id.0)))
                .collect();
            for actor in stayed {
                if revision_view(&before, actor)? != revision_view(&self.game, actor)? {
                    let revision = self.revisions.get_mut(&actor).expect("loaded actor");
                    *revision = revision.checked_add(1).ok_or_else(|| {
                        Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                    })?;
                }
            }
            self.revisions.follow(&self.game)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RevisionView {
    observation: tor_simulation::Observation,
    scene: Vec<tor_world::SightCell>,
    ready: bool,
}

/// Derived views of the committed boundary, never encoded into a save or a
/// rewind snapshot. The mutex preserves Engine's thread-safe read interface.
#[derive(Debug, Default)]
struct ObservationCache(Mutex<BTreeMap<ActorId, Arc<RevisionView>>>);

impl ObservationCache {
    fn view(&self, game: &Game, actor: ActorId) -> Result<Arc<RevisionView>, Failure> {
        let mut views = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(view) = views.get(&actor) {
            return Ok(view.clone());
        }
        let view = Arc::new(revision_view(game, actor)?);
        views.insert(actor, view.clone());
        Ok(view)
    }

    fn snapshot(&self) -> BTreeMap<ActorId, Arc<RevisionView>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn replace(&mut self, views: BTreeMap<ActorId, Arc<RevisionView>>) {
        *self
            .0
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = views;
    }
}

fn revision_view(game: &Game, actor: ActorId) -> Result<RevisionView, Failure> {
    let (view, scene) = game
        .observe_scene(SimActor(actor.0))
        .map_err(|_| invalid_archive())?;
    Ok(RevisionView {
        observation: view,
        scene,
        ready: game.next_actor() == Some(SimActor(actor.0)),
    })
}

/// Durable chronological journal, including retained futures and explicit forks.
#[derive(Debug)]
pub struct Engine {
    recovery: RecoveryProfile,
    current_branch: BranchId,
    wizard_enabled: bool,
    boundaries: VecDeque<Arc<Boundary>>,
    game: Game,
    archive: Archive,
    revisions: Revisions,
    observations: ObservationCache,
    history_index: crate::history_index::HistoryIndex,
    receipts: BTreeMap<(String, String), usize>,
    path: Option<PathBuf>,
    lock: Option<Arc<crate::storage::JournalLock>>,
    store: Option<crate::storage::Store>,
    /// Region records, for games that stream.
    regions: Option<crate::regions::Regions>,
}

impl Engine {
    pub fn scenario_validation(&self) -> Option<bool> {
        self.archive
            .scenario
            .package
            .as_ref()
            .map(|p| p.validated && p.state_valid(&self.game))
    }
    pub fn selected_character(&self) -> Option<ActorId> {
        self.archive
            .scenario
            .package
            .as_ref()
            .map(|p| ActorId(p.selected))
    }
    pub fn memory(scenario: Scenario) -> Result<Self, Failure> {
        if scenario.package.is_none() && !(1..=256).contains(&scenario.regions) {
            return Err(invalid_archive());
        }
        let mut regions = streaming_regions(&scenario)?;
        let mut game = match (scenario.workload_version, &mut regions) {
            (None, Some(regions)) => {
                let package = scenario.package.as_ref().ok_or_else(invalid_archive)?;
                start_streaming(package, scenario.seed, regions)?
            }
            (None, None) => match &scenario.package {
                Some(package) => package.build(scenario.seed, true)?,
                None => scenario_game(&scenario),
            },
            (Some(1), None) => crate::performance_fixture::game(scenario.seed, scenario.regions)?,
            _ => return Err(invalid_archive()),
        };
        let mut revisions = Revisions::new(&game);
        if scenario.actors.is_empty() && scenario.package.is_none() {
            return Err(invalid_archive());
        }
        for actor in &scenario.actors {
            let ticks = NonZeroU64::new(actor.turn_ticks).ok_or_else(invalid_archive)?;
            let id = game
                .spawn_actor(adapt::location(actor.position), ticks)
                .map_err(|_| invalid_archive())?;
            revisions.insert(ActorId(id.0), 0);
        }
        game.refresh_navigation();
        let branch = BranchId(Uuid::new_v4().to_string());
        let initial = Arc::new(Boundary {
            id: None,
            selectable: true,
            game: game.clone(),
            revisions: revisions.clone(),
        });
        Ok(Self {
            recovery: RecoveryProfile::default(),
            current_branch: branch.clone(),
            wizard_enabled: false,
            boundaries: VecDeque::from([initial]),
            game,
            revisions,
            receipts: BTreeMap::new(),
            observations: ObservationCache::default(),
            history_index: crate::history_index::HistoryIndex::default(),
            path: None,
            lock: None,
            store: None,
            regions,
            archive: Archive {
                view_salt: Uuid::new_v4().to_string(),
                wizard_game: false,
                version: ARCHIVE_VERSION,
                ruleset: RULESET.into(),
                scenario,
                branch,
                records: Vec::new(),
            },
        })
    }

    /// Existing saves own their scenario; `scenario` is used only for a new file.
    pub fn open(path: impl AsRef<Path>, scenario: Scenario) -> Result<Self, Failure> {
        Self::open_with_policy(path, scenario, crate::SavePolicy::default())
    }
    pub fn open_with_policy(
        path: impl AsRef<Path>,
        scenario: Scenario,
        policy: crate::SavePolicy,
    ) -> Result<Self, Failure> {
        let started = Instant::now();
        policy.validate()?;
        let (path, lock) = crate::storage::JournalLock::acquire(path.as_ref())?;
        let supplied = scenario.package.clone();
        let (store, archive, checkpoint, saved) = crate::storage::Store::open(
            &path,
            || {
                let engine = Self::memory(scenario)?;
                let sources = engine.unsaved_sources()?;
                Ok((engine.archive, sources))
            },
            policy,
            lock.clone(),
        )?;
        // Region files not copied into the save come from the package
        // directory: the one supplied, or where the save was created, if
        // it's still the same package.
        if let Some(package) = &archive.scenario.package {
            if !saved.is_empty() {
                package
                    .sources
                    .attach_saved(store.source_reader(), saved.clone());
            }
            if !package.sources.has_directory() {
                if let Some(directory) =
                    crate::scenario_package::locate(package, supplied.as_deref())
                {
                    package.sources.set_directory(&directory);
                }
            }
        }
        let records_loaded = archive.records.len();
        let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
        let records_replayed = records_loaded
            - checkpoint
                .as_ref()
                .map(|c| c.record_count)
                .unwrap_or(0)
                .min(records_loaded);
        let mut engine = Self::replay(archive, checkpoint, Some(&store))?;
        if let Some(regions) = &mut engine.regions {
            regions.set_saved(saved);
        }
        engine.require_region_sources()?;
        engine.recovery = RecoveryProfile {
            total: started.elapsed(),
            records_loaded,
            records_replayed,
            checkpoint_sequence,
        };
        engine.path = Some(path);
        engine.lock = Some(lock);
        engine.store = Some(store);
        Ok(engine)
    }
    /// Region files the save needs and doesn't have yet.
    fn unsaved_sources(&self) -> Result<Vec<(u64, Arc<str>)>, Failure> {
        match &self.regions {
            Some(regions) => regions.unsaved_sources(),
            None => Ok(Vec::new()),
        }
    }

    /// A game with regions still to build needs their files: copied into
    /// the save, or in the package directory. Refuse to resume without them
    /// rather than fail when play reaches them.
    fn require_region_sources(&self) -> Result<(), Failure> {
        let (Some(package), Some(regions)) = (&self.archive.scenario.package, &self.regions) else {
            return Ok(());
        };
        if package.sources.has_directory() || !regions.needs_package_files(&self.game) {
            return Ok(());
        }
        Err(Failure::new(
            tor_protocol::ErrorCode::StorageFailure,
            &format!(
                "This save's scenario package isn't available{}; resume with --scenario <directory> naming the same package",
                package
                    .directory
                    .as_ref()
                    .map(|d| format!(" at {d}"))
                    .unwrap_or_default()
            ),
        ))
    }

    /// Build regions and read region rows in the background, just beyond
    /// what the reference points keep loaded. Games play identically with or
    /// without it; replay and recovery don't use it. Does nothing in a game
    /// that doesn't stream.
    pub fn start_preloading(&mut self) {
        if let Some(regions) = &mut self.regions {
            regions.start_preloading();
            regions.preload(&self.game);
        }
    }
    pub fn stop_preloading(&mut self) {
        if let Some(regions) = &mut self.regions {
            regions.stop_preloading();
        }
    }
    /// Wait until the preloader has done what it was asked for. For tests
    /// and benchmarks that measure the prepared path; commands never wait.
    pub fn settle_preloading(&self) {
        if let Some(regions) = &self.regions {
            regions.settle_preloading();
        }
    }
    /// Wait for the records accepted before this call to become durable.
    pub fn flush(&self) -> Result<(), Failure> {
        match &self.store {
            Some(store) => store.flush(),
            None => Ok(()),
        }
    }
    pub fn recovery_profile(&self) -> &RecoveryProfile {
        &self.recovery
    }

    /// Region streaming counts, for games that stream.
    pub fn region_counts(&self) -> Option<RegionCounts> {
        let regions = self.regions.as_ref()?;
        let package = self.archive.scenario.package.as_ref()?;
        let mut counts = RegionCounts {
            resident_records: regions.resident_count(),
            records_read: regions.reads,
            ..RegionCounts::default()
        };
        for region in &package.index.regions {
            match self.game.region_state(tor_world::RegionId(region.id)) {
                Some(tor_simulation::RegionState::Active) => counts.active += 1,
                Some(tor_simulation::RegionState::Frozen) => counts.frozen += 1,
                Some(tor_simulation::RegionState::Detached) => counts.detached += 1,
                // Regions the game hasn't needed yet aren't even declared.
                Some(tor_simulation::RegionState::Unbuilt) | None => counts.unbuilt += 1,
            }
        }
        Some(counts)
    }

    pub fn save_status(&self) -> crate::SaveStatus {
        self.store
            .as_ref()
            .map(|store| store.status())
            .unwrap_or_default()
    }
    pub fn request_save(&self) -> u64 {
        self.store
            .as_ref()
            .map(|store| store.request_flush())
            .unwrap_or(0)
    }
    pub(crate) fn flush_handle(&self) -> Option<crate::storage::Store> {
        self.store.clone()
    }

    fn replay(
        mut archive: Archive,
        checkpoint: Option<DiskCheckpoint>,
        store: Option<&crate::storage::Store>,
    ) -> Result<Self, Failure> {
        if let Some(package) = &archive.scenario.package {
            package.check_identity()?;
        }
        if archive.version != ARCHIVE_VERSION
            || Uuid::parse_str(&archive.view_salt).is_err()
            || archive.ruleset != RULESET
            || Uuid::parse_str(&archive.branch.0).is_err()
        {
            return Err(invalid_archive());
        }
        if checkpoint.is_none() {
            let mut lifecycle = intention_lifecycle::JournalLifecycle::new(archive.branch.clone());
            for record in &archive.records {
                lifecycle.observe(record)?;
            }
        }
        let (mut engine, records) = if let Some(checkpoint) = checkpoint {
            if checkpoint.record_count > archive.records.len() {
                return Err(invalid_archive());
            }
            let records = archive.records.split_off(checkpoint.record_count);
            (checkpoint.restore(archive)?, records)
        } else {
            let records = std::mem::take(&mut archive.records);
            let mut engine = Self::memory(archive.scenario.clone())?;
            engine.current_branch = archive.branch.clone();
            engine.archive = archive;
            (engine, records)
        };
        engine.wizard_enabled = engine.archive.wizard_game;
        // Replay may reattach regions whose records are only on disk.
        if let (Some(regions), Some(store)) = (&mut engine.regions, store) {
            regions.attach_disk(store.clone());
        }
        for record in records {
            if !record.valid_admission()
                || Uuid::parse_str(&record.entry.id.0).is_err()
                || engine.history_index.find(&record.entry.id).is_some()
            {
                return Err(invalid_archive());
            }
            let result = if let Some((source, _)) = record.change_source() {
                let index = engine
                    .history_index
                    .find(source)
                    .ok_or_else(invalid_archive)?;
                if !record.valid_change(&engine.archive.records[index]) {
                    return Err(invalid_archive());
                }
                if let Some(receipt) = record.receipt {
                    engine
                        .apply_command(&receipt, Some(record.entry.id.clone()))
                        .map(|result| result.entry)
                } else if matches!(
                    record.entry.content,
                    JournalContent::IntentionChanged {
                        change: crate::journal::IntentionChange::Cancelled,
                        ..
                    }
                ) {
                    engine
                        .cancel_travel_inner(record.entry.actor, Some(record.entry.id.clone()))?
                        .ok_or_else(invalid_archive)
                        .map(|result| result.entry)
                } else {
                    engine
                        .suspend_intention_admission(
                            record.entry.actor,
                            source,
                            Some(record.entry.id.clone()),
                        )
                        .map(|result| result.entry)
                }
            } else if record.resolution().is_some() {
                let (source, _) = record.resolution().expect("checked resolution");
                let index = engine
                    .history_index
                    .find(source)
                    .ok_or_else(invalid_archive)?;
                if !record.valid_resolution(&engine.archive.records[index]) {
                    return Err(invalid_archive());
                }
                engine
                    .execute_intention(Some(record.entry.id.clone()))?
                    .ok_or_else(invalid_archive)
                    .map(|result| result.entry)
            } else if let JournalContent::TravelIntentionAdmitted {
                journey,
                step,
                action,
                destination,
                ..
            } = &record.entry.content
            {
                let Action::Move { direction } = action else {
                    return Err(invalid_archive());
                };
                engine
                    .admit_travel_inner(
                        record.entry.actor,
                        journey,
                        *step,
                        tor_simulation::TravelStep {
                            direction: adapt::simulation_direction(*direction),
                            destination: *destination,
                        },
                        Some(record.entry.id.clone()),
                    )
                    .map(|result| result.entry)
            } else if matches!(
                record.entry.content,
                JournalContent::AutonomousIntentionAdmitted { .. }
            ) {
                engine
                    .admit_ai_inner(record.entry.actor, Some(record.entry.id.clone()), None)
                    .map(|result| result.entry)
            } else if let Some(receipt) = record.receipt {
                engine
                    .apply_command(&receipt, Some(record.entry.id.clone()))
                    .map(|result| result.entry)
            } else if let (
                Author::Backend { component },
                JournalContent::Annotation {
                    anchor,
                    category,
                    text,
                },
            ) = (&record.entry.author, &record.entry.content)
            {
                engine.backend_note(
                    record.entry.actor,
                    component,
                    anchor.clone(),
                    *category,
                    text,
                    Some(record.entry.id.clone()),
                )
            } else {
                return Err(invalid_archive());
            };
            if result.map_err(|_| invalid_archive())? != record.entry {
                return Err(invalid_archive());
            }
        }
        engine.wizard_enabled = false;
        Ok(engine)
    }

    pub fn branch(&self) -> &BranchId {
        &self.current_branch
    }
    /// Trusted administration operation. The marker commits before authority changes.
    pub fn enable_wizard(&mut self) -> Result<(), Failure> {
        if !self.archive.wizard_game {
            if let Some(store) = &self.store {
                store.wizard()?;
            }
            // Once promotion is admitted it cannot be cleared, even if the
            // following durable barrier fails. Authority remains disabled.
            self.archive.wizard_game = true;
        }
        self.flush()?;
        self.wizard_enabled = true;
        Ok(())
    }
    pub fn wizard_enabled(&self) -> bool {
        self.wizard_enabled
    }
    /// Actors in loaded regions. Revisions also cover actors in regions that
    /// are detached or not built yet.
    pub fn actors(&self) -> Vec<ActorId> {
        self.revisions.keys().copied().collect()
    }
    pub fn is_ai(&self, actor: ActorId) -> bool {
        self.game.is_ai(SimActor(actor.0))
    }
    /// The actor the scheduler runs next, if any can act.
    pub fn next_actor(&self) -> Option<ActorId> {
        self.game.next_actor().map(|actor| ActorId(actor.0))
    }
    pub fn alive(&self, actor: ActorId) -> bool {
        self.game.alive(SimActor(actor.0))
    }

    pub fn pause_preparation(&mut self, actor: ActorId) -> Result<Option<CommandResult>, Failure> {
        if self.game.is_ai(SimActor(actor.0))
            || self
                .game
                .preparation(SimActor(actor.0))
                .is_none_or(|p| !p.active)
        {
            return Ok(None);
        }
        if let Some(intention) = self
            .game
            .preparation(SimActor(actor.0))
            .and_then(|p| p.intention)
        {
            let source = self
                .history_index
                .intention_admission(intention)
                .ok_or_else(invalid_archive)?;
            let admission = self.archive.records[source].entry.id.clone();
            return self
                .suspend_intention_admission(actor, &admission, None)
                .map(Some);
        }
        self.command(
            "scheduler",
            "server",
            actor,
            &Uuid::new_v4().to_string(),
            &self.branch().clone(),
            Command::PausePreparation,
        )
        .map(Some)
    }

    pub fn next_ai_action(&self) -> Option<(ActorId, Action)> {
        let (actor, action) = self.game.next_ai_action()?;
        let action = adapt::recorded_action(action)?;
        Some((ActorId(actor.0), action))
    }

    /// Convenience driver: admit a decision, then execute it through the queue.
    /// Session journals admission and execution as separate boundaries.
    pub fn advance_ai(&mut self, actor: ActorId) -> Result<CommandResult, Failure> {
        self.ensure_ai_admission(actor, None)?;
        self.execute_intention(None)?.ok_or_else(invalid_archive)
    }

    pub fn advance_ai_profiled(
        &mut self,
        actor: ActorId,
    ) -> Result<(CommandResult, CommandProfile), Failure> {
        let mut profile = CommandProfile::default();
        self.ensure_ai_admission(actor, Some(&mut profile))?;
        let result = self
            .execute_intention_profiled(None, Some(&mut profile))?
            .ok_or_else(invalid_archive)?;
        Ok((result, profile))
    }

    pub fn revision(&self, actor: ActorId) -> Result<u64, Failure> {
        self.revisions
            .any(actor, &self.game)
            .ok_or_else(|| Failure::new(ErrorCode::Unauthorized, "Actor is unavailable"))
    }
    pub fn observation(&self, actor: ActorId) -> Result<Observation, Failure> {
        self.revision(actor)?;
        let view = self.revision_view(actor)?;
        let package = self.archive.scenario.package.as_deref();
        let terrain = |region: tor_world::RegionId| {
            package
                .and_then(|p| p.region_terrain(region.0))
                .map_or([None, None, None], |t| {
                    [t.floor.clone(), t.wall.clone(), t.door.clone()]
                })
        };
        let mut observation = adapt::observation(
            view.observation.clone(),
            view.scene.clone(),
            &self.archive.view_salt,
            view.ready,
            &terrain,
            &self.target_scope(actor),
        );
        observation.places = self
            .game
            .remembered_places(SimActor(actor.0))
            .map(|(location, name, origin)| PlaceView {
                key: adapt::cell_key(&self.archive.view_salt, actor.0, location),
                name: name.into(),
                origin: match origin {
                    tor_simulation::NameOrigin::Invented => PlaceNameOrigin::Invented,
                    tor_simulation::NameOrigin::Authored => PlaceNameOrigin::Authored,
                    tor_simulation::NameOrigin::Player => PlaceNameOrigin::Player,
                },
            })
            .collect();
        Ok(observation)
    }
    fn revision_view(&self, actor: ActorId) -> Result<Arc<RevisionView>, Failure> {
        self.observations.view(&self.game, actor)
    }

    /// The assets an actor's client may soon need: those of the themes of
    /// every region within one portal hop beyond what's kept loaded around
    /// it, from the package's structure alone, never from what the regions
    /// hold. A game that doesn't stream keeps every region loaded, so its
    /// palette covers the whole package. `None` when the scenario names no
    /// assets.
    pub fn palette(&self, actor: ActorId) -> Option<std::collections::BTreeSet<String>> {
        let package = self
            .archive
            .scenario
            .package
            .as_deref()
            .filter(|p| p.has_assets())?;
        let region = self.palette_region(actor)?;
        Some(match &self.regions {
            Some(regions) => regions.palette(package, tor_world::RegionId(region)),
            None => package.palette(
                package
                    .index
                    .regions
                    .iter()
                    .flat_map(|r| package.region_themes(r.id).unwrap_or(&[])),
            ),
        })
    }

    /// The region an actor's palette is forecast from. A palette depends on
    /// nothing else, so clients recompute it only when this changes.
    pub fn palette_region(&self, actor: ActorId) -> Option<u64> {
        self.game
            .known_actor_region(SimActor(actor.0))
            .map(|region| region.0)
    }

    pub fn travel_route(
        &self,
        actor: ActorId,
        destination: &str,
    ) -> Result<Vec<tor_simulation::TravelStep>, Failure> {
        let unavailable = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Travel destination or known route is unavailable",
            )
        };
        let location = self
            .game
            .known_cells(SimActor(actor.0))
            .find(|&cell| adapt::cell_key(&self.archive.view_salt, actor.0, cell) == destination)
            .ok_or_else(unavailable)?;
        self.game
            .travel_route(SimActor(actor.0), location)
            .map_err(|_| unavailable())
    }

    pub fn state(&self, actor: ActorId) -> Result<StateView, Failure> {
        Ok(StateView {
            wizard_game: self.archive.wizard_game,
            revision: self.revision(actor)?,
            observation: self.observation(actor)?,
        })
    }

    pub fn history(
        &self,
        actor: ActorId,
        user: &str,
        before: Option<&EntryId>,
        limit: usize,
    ) -> Result<HistoryPage, Failure> {
        self.history_branch(actor, user, self.branch(), before, limit)
    }

    pub fn history_branch(
        &self,
        actor: ActorId,
        user: &str,
        branch: &BranchId,
        before: Option<&EntryId>,
        limit: usize,
    ) -> Result<HistoryPage, Failure> {
        self.revision(actor)?;
        if limit == 0 || limit > MAX_HISTORY_PAGE {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid history page size",
            ));
        }
        let end = match before {
            Some(id) => {
                let position = self.history_index.find(id).ok_or_else(invalid_anchor)?;
                let entry = &self.archive.records[position].entry;
                #[cfg(test)]
                HISTORY_RECORD_VISITS.with(|visits| visits.set(visits.get() + 1));
                if &entry.branch != branch || !entry.visible_to(actor, user) {
                    return Err(invalid_anchor());
                }
                position
            }
            None => self.archive.records.len(),
        };
        let (positions, older) = self.history_index.page(actor, user, branch, end, limit);
        #[cfg(test)]
        HISTORY_RECORD_VISITS.with(|visits| visits.set(visits.get() + positions.len()));
        Ok(HistoryPage {
            entries: positions
                .iter()
                .filter_map(|&position| self.disclose_entry(&self.archive.records[position].entry))
                .collect(),
            older_before: older.then(|| self.archive.records[positions[0]].entry.id.clone()),
        })
    }

    /// Recover a durable receipt before checking ephemeral control ownership.
    pub fn retry(
        &self,
        user: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: &Command,
    ) -> Result<Option<CommandResult>, Failure> {
        self.retry_matching(user, actor, request_id, branch, |original| {
            original == command
        })
    }

    fn retry_matching(
        &self,
        user: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        matches: impl FnOnce(&Command) -> bool,
    ) -> Result<Option<CommandResult>, Failure> {
        let Some(&index) = self.receipts.get(&(user.into(), request_id.into())) else {
            return Ok(None);
        };
        let record = &self.archive.records[index];
        let receipt = record
            .receipt
            .as_ref()
            .expect("receipt index only contains client records");
        if receipt.actor != actor || &receipt.branch != branch || !matches(&receipt.command) {
            return Err(Failure::new(
                ErrorCode::RequestConflict,
                "Request ID was already used for another command",
            ));
        }
        Ok(Some(CommandResult {
            entry: record.entry.clone(),
            duplicate: true,
        }))
    }

    pub fn command(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
            None,
        )
    }

    /// Execute the production command path while collecting phase diagnostics.
    pub fn command_profiled(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
    ) -> Result<(CommandResult, CommandProfile), Failure> {
        let started = Instant::now();
        let counts = tor_simulation::diagnostics::work_counts();
        let mut profile = CommandProfile::default();
        let result = self.apply_command_profiled(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
            Some(&mut profile),
        )?;
        profile.authoritative_total = started.elapsed();
        let after = tor_simulation::diagnostics::work_counts();
        profile.perception_calls = after.observations - counts.observations;
        profile.scene_calls = after.scenes - counts.scenes;
        Ok((result, profile))
    }

    /// (retained records, retained rewind boundaries), without cloning either.
    pub fn profile_counts(&self) -> (usize, usize) {
        (self.archive.records.len(), self.boundaries.len())
    }

    /// Attach a detached, seeded fixture to a new, exclusively locked save.
    /// Setup persists before any timed ordinary command can publish state.
    pub fn attach_profile_save(self, path: impl AsRef<Path>) -> Result<Self, Failure> {
        self.attach_profile_save_with_policy(path, crate::SavePolicy::default())
    }
    pub fn attach_profile_save_with_policy(
        mut self,
        path: impl AsRef<Path>,
        policy: crate::SavePolicy,
    ) -> Result<Self, Failure> {
        if self.path.is_some() {
            return Err(storage_failure());
        }
        let (path, lock) = crate::storage::JournalLock::acquire(path.as_ref())?;
        if path.exists() {
            return Err(storage_failure());
        }
        let sources = self.unsaved_sources()?;
        let (store, _, _, saved) = crate::storage::Store::open(
            &path,
            || Ok((self.archive.clone(), sources)),
            policy,
            lock.clone(),
        )?;
        // Records evicted once a checkpoint writes them are read back from it.
        if let Some(regions) = &mut self.regions {
            regions.attach_disk(store.clone());
            regions.set_saved(saved);
        }
        self.path = Some(path);
        self.lock = Some(lock);
        self.store = Some(store);
        Ok(self)
    }

    /// Measure the current format-6 checkpoint JSON without allocating its encoded
    /// payload or writing a save. Includes capture, deduplication and serialization;
    /// intended for offline diagnostics, outside measured action intervals. The
    /// production 64 MiB writer limit remains enforced independently.
    pub fn profile_checkpoint_encoding(&self) -> Result<(u64, Duration), Failure> {
        struct Counter(u64);
        impl std::io::Write for Counter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 += bytes.len() as u64;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let start = Instant::now();
        let snapshot = Checkpoint::capture_candidate(
            &Candidate::capture(self),
            self.archive.records.len(),
            self.archive.wizard_game,
        );
        let encoded = snapshot.encode(
            "00000000-0000-0000-0000-000000000000",
            self.archive.records.len() as u64,
        );
        let mut counter = Counter(0);
        serde_json::to_writer(&mut counter, &encoded).map_err(|_| storage_failure())?;
        Ok((counter.0, start.elapsed()))
    }

    /// Persist the current archive to a disposable diagnostic target. The returned
    /// duration is complete bootstrap work, not command encoding or physical I/O.
    /// This does not attach the target to the engine or publish state.
    pub fn profile_persistence(&self, path: impl AsRef<Path>) -> Result<BootstrapProfile, Failure> {
        let started = Instant::now();
        let mut candidate = self.diagnostic_copy();
        candidate.path = None;
        candidate.lock = None;
        candidate.store = None;
        let candidate = candidate.attach_profile_save(path)?;
        candidate.flush()?;
        Ok(BootstrapProfile {
            total: started.elapsed(),
            records_serialized: self.archive.records.len(),
        })
    }

    /// Populate an in-memory diagnostic fixture in linear time. Records are
    /// ordinary deterministic wait actions and remain replayable; persistence
    /// is intentionally bypassed so a 10,000-action baseline does not spend its
    /// setup time exercising the quadratic behavior being measured.
    pub fn seed_profile_history(&mut self, count: usize) -> Result<(), Failure> {
        if self.path.is_some() || self.lock.is_some() {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Only detached fixtures may be seeded",
            ));
        }
        let start = self.archive.records.len();
        let end = start.checked_add(count).ok_or_else(invalid_archive)?;
        if count > 0 {
            self.observations.replace(BTreeMap::new());
        }
        if count >= REWIND_BOUNDARIES {
            self.boundaries.clear();
        }
        for index in start..end {
            let actor = ActorId(self.game.next_actor().ok_or_else(invalid_archive)?.0);
            let expected_revision = self.revision(actor)?;
            let tick = self.game.tick();
            let outcome = self
                .game
                .act(SimActor(actor.0), tor_simulation::Action::Wait)
                .map_err(|_| invalid_archive())?;
            // Wait changes only time/readiness. Geometry and navigation are
            // identical; equivalence tests compare the complete Game and every
            // retained boundary against ordinary commands. Never use this
            // shortcut for measured actions or an engine attached to a save.
            for (&id, revision) in self.revisions.iter_mut() {
                if outcome.next_tick != tick
                    || ((id == actor) != (outcome.next_actor == Some(SimActor(id.0))))
                {
                    *revision = revision.checked_add(1).ok_or_else(invalid_archive)?;
                }
            }
            let entry = JournalEntry {
                intention_suspensions: Vec::new(),
                intention_ends: Vec::new(),
                id: new_id(),
                branch: self.branch().clone(),
                actor,
                tick,
                author: Author::User {
                    user: "bench".into(),
                },
                audience: Audience::Actor,
                content: JournalContent::Action {
                    action: Action::Wait,
                    event: adapt::event(outcome.kind),
                },
            };
            let receipt = Receipt {
                user: "bench".into(),
                frontend: "headless".into(),
                request_id: format!("seed-{index}"),
                actor,
                branch: self.branch().clone(),
                command: Command::Act {
                    expected_revision,
                    action: Action::Wait,
                },
            };
            self.append_record(Record {
                entry: entry.clone(),
                receipt: Some(receipt),
            });
            if end - index <= REWIND_BOUNDARIES {
                self.boundaries.push_back(Arc::new(Boundary {
                    id: Some(entry.id),
                    selectable: true,
                    game: self.game.clone(),
                    revisions: self.revisions.clone(),
                }));
            }
            retain_boundaries(&mut self.boundaries, |b| b.selectable);
        }
        Ok(())
    }

    fn apply_command(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled(receipt, recorded_id, None)
    }

    fn apply_command_profiled(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
        profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        if let Some(result) = self.resolve_receipt(receipt)? {
            return Ok(result);
        }
        let checked = self.check_request(receipt)?;
        self.execute_checked_command(checked, recorded_id, profile)
    }

    /// Consumes metadata checked at this boundary without a callback or yield.
    /// Target/timing validation, candidate execution, persistence admission and
    /// publication remain ordered inside this private operation.
    fn execute_checked_command(
        &mut self,
        checked: command_request::CheckedRequest<'_>,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        let candidate = self.capture_command_candidate(profile.as_deref_mut());
        self.finish_checked_command(checked, recorded_id, profile, candidate)
    }

    fn capture_command_candidate(&mut self, profile: Option<&mut CommandProfile>) -> Candidate {
        if let (Some(regions), Some(store)) = (&mut self.regions, &self.store) {
            if let Some((on_disk, watermark)) = store.take_written() {
                regions.written(on_disk, watermark);
            }
        }
        let started = Instant::now();
        let candidate = Candidate::capture(self);
        if let Some(profile) = profile {
            profile.rollback_capture += started.elapsed();
            profile.candidate_captures += 1;
        }
        candidate
    }

    /// Apply/reconcile one action inside a private candidate. A supplied outcome
    /// comes only from an uninterrupted simulation execution against that candidate.
    fn resolve_action_transition(
        &self,
        candidate: &mut Candidate,
        actor: ActorId,
        action: &Action,
        executed: Option<tor_simulation::ActionOutcome>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<tor_simulation::ActionOutcome, Failure> {
        let tick = self.game.tick();
        let mut navigation_refreshed = false;
        let navigation_changed = match action {
            Action::Move { .. } | Action::SetDoor { .. } => true,
            Action::Attack { .. } => false,
            Action::Wait => self.game.wait_changes_perception(SimActor(actor.0)),
            Action::Take { .. } | Action::Drop { .. } => self.game.physics_enabled(),
        };
        let started = Instant::now();
        let perception_changed = match action {
            Action::Wait => self.game.wait_changes_perception(SimActor(actor.0)),
            Action::Move { .. }
            | Action::Attack { .. }
            | Action::SetDoor { .. }
            | Action::Take { .. }
            | Action::Drop { .. } => true,
        };
        let before: BTreeMap<_, _> = self
            .actors()
            .into_iter()
            .filter(|_| perception_changed)
            .map(|actor| Ok((actor, self.revision_view(actor)?)))
            .collect::<Result<_, Failure>>()?;
        if let Some(profile) = profile.as_deref_mut() {
            profile.perception += started.elapsed();
            profile.actors_observed += before.len();
        }
        let started = Instant::now();
        let already_executed = executed.is_some();
        let outcome = match executed {
            Some(outcome) => outcome,
            None => candidate
                .game
                .act(SimActor(actor.0), adapt::action(action))
                .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Action is unavailable"))?,
        };
        if let Some(profile) = profile.as_deref_mut().filter(|_| !already_executed) {
            profile.simulation_transition += started.elapsed();
            profile.simulation_transitions += 1;
        }
        // Wait changes only tick/readiness. Other outcomes retain full
        // comparison; new action kinds must make their impact explicit.
        if !perception_changed {
            let started = Instant::now();
            for (&observer, revision) in candidate.revisions.iter_mut() {
                if outcome.next_tick != tick
                    || ((observer == actor) != (outcome.next_actor == Some(SimActor(observer.0))))
                {
                    *revision = revision.checked_add(1).ok_or_else(|| {
                        Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                    })?;
                }
                if let Some(profile) = profile.as_deref_mut() {
                    profile.revision_comparisons += 1;
                }
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.revision_detection += started.elapsed();
            }
        }
        for (actor, old) in before {
            let perception_started = Instant::now();
            let after = candidate.revision_view(actor)?;
            if let Some(profile) = profile.as_deref_mut() {
                profile.perception += perception_started.elapsed();
                profile.actors_observed += 1;
            }
            if navigation_changed || after.scene != old.scene {
                navigation_refreshed = true;
                let started = Instant::now();
                candidate
                    .game
                    .refresh_navigation_scene(SimActor(actor.0), &after.scene);
                if let Some(profile) = profile.as_deref_mut() {
                    profile.navigation_refresh += started.elapsed();
                }
            }
            let started = Instant::now();
            let changed = after != *old;
            if changed {
                let revision = candidate.revisions.get_mut(&actor).expect("known actor");
                *revision = revision
                    .checked_add(1)
                    .ok_or_else(|| Failure::new(ErrorCode::InvalidAction, "Revision exhausted"))?;
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.revision_detection += started.elapsed();
                profile.revision_comparisons += 1;
            }
            candidate.observations.insert(actor, Arc::new(after));
        }
        if navigation_refreshed {
            if let Some(profile) = profile {
                profile.navigation_refreshes += 1;
            }
        }
        Ok(outcome)
    }

    fn finish_checked_command(
        &mut self,
        checked: command_request::CheckedRequest<'_>,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
        mut candidate: Candidate,
    ) -> Result<CommandResult, Failure> {
        let (receipt, revision) = checked.into_parts();
        let mut tick = self.game.tick();
        let entry_id = recorded_id.unwrap_or_else(new_id);
        let (author, audience, content) = match &receipt.command {
            Command::ResumeIntention { admission, .. }
            | Command::CancelIntention { admission, .. } => {
                let change = if matches!(receipt.command, Command::ResumeIntention { .. }) {
                    crate::journal::IntentionChange::Resumed
                } else {
                    crate::journal::IntentionChange::Cancelled
                };
                let content = self.prepare_intention_change(
                    &mut candidate,
                    receipt.actor,
                    admission,
                    change,
                )?;
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Private,
                    content,
                )
            }
            Command::AdmitIntention { action, .. } => {
                let intention = candidate
                    .game
                    .admit_intention(
                        SimActor(receipt.actor.0),
                        adapt::action(action),
                        tor_simulation::IntentionOrigin::Human,
                    )
                    .map_err(|error| {
                        Failure::new(
                            if matches!(error, tor_simulation::GameError::ActorBusy) {
                                ErrorCode::ActorBusy
                            } else {
                                ErrorCode::InvalidAction
                            },
                            "Intention is unavailable",
                        )
                    })?;
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Private,
                    JournalContent::IntentionAdmitted {
                        intention,
                        action: action.clone(),
                    },
                )
            }
            Command::PausePreparation => {
                let target = candidate
                    .game
                    .pause_preparation(SimActor(receipt.actor.0))
                    .ok_or_else(|| {
                        Failure::new(ErrorCode::InvalidAction, "No active preparation")
                    })?;
                for revision in candidate.revisions.values_mut() {
                    *revision = revision.checked_add(1).ok_or_else(|| {
                        Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                    })?;
                }
                (
                    Author::Backend {
                        component: "scheduler".into(),
                    },
                    Audience::Actor,
                    JournalContent::Action {
                        action: Action::Attack { target },
                        event: crate::journal::Event::PreparationPaused,
                    },
                )
            }
            Command::RenamePlace {
                expected_revision: _,
                key,
                name,
            } => {
                let location = self
                    .game
                    .remembered_places(SimActor(receipt.actor.0))
                    .find(|(location, ..)| {
                        adapt::cell_key(&self.archive.view_salt, receipt.actor.0, *location) == *key
                    })
                    .map(|(location, ..)| location)
                    .ok_or_else(|| {
                        Failure::new(ErrorCode::InvalidRequest, "Place or name is unavailable")
                    })?;
                candidate
                    .game
                    .rename_place(SimActor(receipt.actor.0), location, name)
                    .map_err(|_| {
                        Failure::new(ErrorCode::InvalidRequest, "Place or name is unavailable")
                    })?;
                *candidate
                    .revisions
                    .get_mut(&receipt.actor)
                    .expect("known actor") = revision
                    .checked_add(1)
                    .ok_or_else(|| Failure::new(ErrorCode::InvalidRequest, "Revision exhausted"))?;
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    JournalContent::PlaceRenamed {
                        key: key.clone(),
                        name: name.clone(),
                    },
                )
            }

            Command::Travel {
                expected_revision: _,
                destination,
            } => {
                self.travel_route(receipt.actor, destination)?;
                if self.game.next_actor() != Some(SimActor(receipt.actor.0)) {
                    return Err(Failure::new(ErrorCode::InvalidAction, "Actor is not ready"));
                }
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    JournalContent::Travel {
                        destination: destination.clone(),
                    },
                )
            }

            Command::Wizard {
                expected_revision: _,
                operation,
            } => {
                candidate.load_for_wizard(self.regions.as_mut(), operation)?;
                let result =
                    candidate.apply_wizard(receipt, operation, &entry_id, &self.archive.records)?;
                tick = candidate.game.tick();
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Private,
                    JournalContent::Wizard {
                        operation: operation.clone(),
                        validation: self
                            .archive
                            .scenario
                            .package
                            .as_ref()
                            .map(|p| p.validated && p.state_valid(&candidate.game)),
                        result,
                    },
                )
            }
            Command::Act {
                expected_revision: _,
                action,
            } => {
                let outcome = self.resolve_action_transition(
                    &mut candidate,
                    receipt.actor,
                    action,
                    None,
                    profile.as_deref_mut(),
                )?;
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    JournalContent::Action {
                        action: action.clone(),
                        event: adapt::event(outcome.kind),
                    },
                )
            }
            Command::Annotate {
                anchor,
                text,
                source,
                audience,
                category,
            } => {
                self.validate_note(receipt.actor, &receipt.user, *audience, anchor, text)?;
                let author = match source {
                    ClientSource::User => Author::User {
                        user: receipt.user.clone(),
                    },
                    ClientSource::Frontend => Author::Frontend {
                        user: receipt.user.clone(),
                        component: receipt.frontend.clone(),
                    },
                };
                (
                    author,
                    *audience,
                    JournalContent::Annotation {
                        anchor: anchor.clone(),
                        category: *category,
                        text: text.clone(),
                    },
                )
            }
        };
        if !matches!(
            receipt.command,
            Command::AdmitIntention { .. }
                | Command::ResumeIntention { .. }
                | Command::CancelIntention { .. }
        ) {
            candidate.transition(self.regions.as_mut(), profile.as_deref_mut())?;
        }
        let started = Instant::now();
        if matches!(receipt.command, Command::Wizard { .. }) {
            candidate.game.refresh_navigation();
            if let Some(profile) = profile.as_deref_mut() {
                profile.navigation_refresh += started.elapsed();
                profile.navigation_refreshes += 1;
            }
        }
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: entry_id,
            branch: candidate.branch().clone(),
            actor: receipt.actor,
            tick,
            author,
            audience,
            content,
        };
        self.commit_candidate(candidate, entry, Some(receipt.clone()), profile)
    }

    fn commit_candidate(
        &mut self,
        mut candidate: Candidate,
        mut entry: JournalEntry,
        receipt: Option<Receipt>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        entry.intention_suspensions = intention::derive_intention_suspensions(
            &self.game,
            &candidate.game,
            &entry,
            candidate.branch() == self.branch(),
        );
        entry.intention_ends = intention::derive_intention_ends(
            &self.game,
            &candidate.game,
            &entry,
            candidate.branch() == self.branch(),
        );
        let record = Record {
            entry: entry.clone(),
            receipt,
        };
        if !matches!(entry.content, JournalContent::Annotation { .. }) {
            let started = Instant::now();
            candidate.boundaries.push_back(Arc::new(Boundary {
                id: Some(entry.id.clone()),
                selectable: entry.content.rewindable(),
                game: candidate.game.clone(),
                revisions: candidate.revisions.clone(),
            }));
            retain_boundaries(&mut candidate.boundaries, |b| b.selectable);
            if let Some(profile) = profile.as_deref_mut() {
                profile.rollback_snapshot += started.elapsed();
                profile.rollback_snapshots += 1;
            }
        }
        // Admission changes queue identity only. Already-built committed views
        // remain valid across this boundary and execution can reuse them.
        if entry.content.admission().is_some() {
            candidate.observations = self.observations.snapshot();
        }
        let copied = self.admit(&record, &candidate, profile.as_deref_mut())?;
        let started = Instant::now();
        let preload = candidate.publish(self);
        self.mark_saved(copied);
        if let (Some(profile), Some(preload)) = (profile.as_deref_mut(), preload) {
            profile.preload_jobs += preload.jobs.len();
            profile.preload_regions_expanded += preload.regions_expanded;
            profile.preload_links_examined += preload.links_examined;
        }
        self.append_record(record);
        if let Some(profile) = profile {
            profile.publication += started.elapsed();
        }
        Ok(CommandResult {
            entry,
            duplicate: false,
        })
    }

    pub fn annotate_backend(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
    ) -> Result<JournalEntry, Failure> {
        self.backend_note(actor, component, anchor, category, text, None)
    }

    fn backend_note(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
        recorded_id: Option<EntryId>,
    ) -> Result<JournalEntry, Failure> {
        if !valid_label(component) {
            return Err(Failure::new(
                ErrorCode::InvalidAnnotation,
                "Invalid backend component",
            ));
        }
        self.validate_note(actor, "", Audience::Actor, &anchor, text)?;
        let entry = JournalEntry {
            intention_suspensions: Vec::new(),
            intention_ends: Vec::new(),
            id: recorded_id.unwrap_or_else(new_id),
            branch: self.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: component.into(),
            },
            audience: Audience::Actor,
            content: JournalContent::Annotation {
                anchor,
                category,
                text: text.into(),
            },
        };
        let record = Record {
            entry: entry.clone(),
            receipt: None,
        };
        let copied = self.admit(&record, &Candidate::capture(self), None)?;
        self.mark_saved(copied);
        self.append_record(record);
        Ok(entry)
    }

    fn validate_note(
        &self,
        actor: ActorId,
        user: &str,
        audience: Audience,
        anchor: &Anchor,
        text: &str,
    ) -> Result<(), Failure> {
        let revision = self.revision(actor)?;
        if text.trim().is_empty()
            || text.len() > MAX_NOTE_BYTES
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(Failure::new(
                ErrorCode::InvalidAnnotation,
                "Notes require 1â€“4096 bytes of plain text",
            ));
        }
        match anchor {
            Anchor::State { revision: target } if *target <= revision => Ok(()),
            Anchor::Entry { id } => {
                let position = self.history_index.find(id).ok_or_else(invalid_anchor)?;
                let entry = &self.archive.records[position].entry;
                if &entry.branch != self.branch()
                    || !entry.visible_to(actor, user)
                    || (audience == Audience::Actor && entry.audience == Audience::Private)
                {
                    return Err(invalid_anchor());
                }
                Ok(())
            }
            _ => Err(invalid_anchor()),
        }
    }

    fn append_record(&mut self, record: Record) {
        let position = self.archive.records.len();
        self.history_index.append(&record.entry, position);
        if let Some(receipt) = &record.receipt {
            self.receipts
                .insert((receipt.user.clone(), receipt.request_id.clone()), position);
        }
        self.archive.records.push(record);
    }

    // Full history copies are confined to detached diagnostics. Ordinary command
    // transactions use Candidate, which cannot own history or receipt indexes.
    fn diagnostic_copy(&self) -> Self {
        Self {
            recovery: self.recovery.clone(),
            current_branch: self.current_branch.clone(),
            wizard_enabled: self.wizard_enabled,
            boundaries: self.boundaries.clone(),
            game: self.game.clone(),
            archive: self.archive.clone(),
            revisions: self.revisions.clone(),
            receipts: self.receipts.clone(),
            observations: ObservationCache::default(),
            history_index: self.history_index.clone(),
            path: self.path.clone(),
            lock: self.lock.clone(),
            store: self.store.clone(),
            regions: self.regions.clone(),
        }
    }

    /// Queue a record, with the region files its command (or earlier ones)
    /// built regions from that the save doesn't have yet. Returns those
    /// regions, for the caller to mark saved once the command publishes.
    fn admit(
        &self,
        record: &Record,
        candidate: &Candidate,
        profile: Option<&mut CommandProfile>,
    ) -> Result<Vec<u64>, Failure> {
        let mut copied = Vec::new();
        if let Some(store) = &self.store {
            let started = Instant::now();
            let mut capture_time = Duration::ZERO;
            let mut captures = 0;
            let sources = match &self.regions {
                Some(regions) => regions.unsaved_sources()?,
                None => Vec::new(),
            };
            copied = sources.iter().map(|(region, _)| *region).collect();
            store.enqueue(record, sources, || {
                let capture_started = Instant::now();
                let mut checkpoint = Checkpoint::capture_candidate(
                    candidate,
                    self.archive.records.len() + 1,
                    self.archive.wizard_game,
                );
                if let Some(regions) = &self.regions {
                    checkpoint.add_records(regions, &candidate.made);
                }
                capture_time = capture_started.elapsed();
                captures = 1;
                checkpoint
            })?;
            if let Some(profile) = profile {
                profile.journal_serialization += started.elapsed() - capture_time;
                profile.checkpoint_capture += capture_time;
                profile.checkpoint_captures += captures;
                profile.records_serialized += 1;
            }
        }
        Ok(copied)
    }

    fn mark_saved(&mut self, copied: Vec<u64>) {
        if let Some(regions) = &mut self.regions {
            regions.mark_saved(copied);
        }
    }
}

/// The region store for a scenario that streams.
pub(crate) fn streaming_regions(
    scenario: &Scenario,
) -> Result<Option<crate::regions::Regions>, Failure> {
    match (&scenario.package, scenario.streaming) {
        (Some(package), Some(streaming)) if scenario.workload_version.is_none() => Ok(Some(
            crate::regions::Regions::new(package.clone(), scenario.seed, streaming)?,
        )),
        (_, None) => Ok(None),
        _ => Err(invalid_archive()),
    }
}

/// A package game with no region built, given default reference points and
/// moved to the regions they need.
fn start_streaming(
    package: &crate::scenario_package::Package,
    seed: u64,
    regions: &mut crate::regions::Regions,
) -> Result<Game, Failure> {
    let mut game = package.start(seed)?;
    let observe = |id: u64| tor_simulation::ReferencePoint {
        target: tor_simulation::ReferenceTarget::Actor(SimActor(id)),
        active_radius: None,
        load_radius: None,
        observes: true,
    };
    // Characters get points by default. A package without combat has no run
    // characters, so its selected character gets one instead.
    let points = game
        .add_default_reference_points()
        .map_err(|_| invalid_archive())?;
    if points.is_empty() && game.reference_points().next().is_none() {
        game.add_reference_point(observe(package.selected))
            .map_err(|_| invalid_archive())?;
    }
    // So do actors that clients control: their players see from them.
    for actor in package.index.regions.iter().flat_map(|r| &r.actors) {
        if actor.external {
            game.add_reference_point(observe(actor.id))
                .map_err(|_| invalid_archive())?;
        }
    }
    // Declaring a generated region read its file, so replay needs it.
    regions.need_files(
        game.unbuilt_regions()
            .map(|r| r.0)
            .filter(|r| package.declaring_reads_file(*r)),
    );
    let mut made = Vec::new();
    regions.transition(&mut game, &mut made)?;
    regions.publish(made);
    Ok(game)
}

pub(crate) fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn new_id() -> EntryId {
    EntryId(Uuid::new_v4().to_string())
}
fn invalid_anchor() -> Failure {
    Failure::new(ErrorCode::InvalidAnchor, "Annotation target is unavailable")
}
pub(crate) fn storage_failure() -> Failure {
    Failure::new(
        ErrorCode::StorageFailure,
        "Could not commit the game journal",
    )
}
pub(crate) fn invalid_archive() -> Failure {
    Failure::new(
        ErrorCode::InvalidArchive,
        "Unsupported or inconsistent game journal",
    )
}

#[cfg(test)]
mod save_lock_tests {
    use super::*;

    #[test]
    fn last_journal_owner_releases_the_lock_even_if_an_inherited_descriptor_survives() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ownership.db");
        let engine = Engine::open(&path, Scenario::two_room(0)).unwrap();
        let owner = engine.lock.as_ref().unwrap().clone();
        // A descriptor inherited across a process spawn is not another Rust owner.
        let inherited = owner.duplicate_descriptor().unwrap();
        drop(engine);
        assert!(Engine::open(&path, Scenario::two_room(0)).is_err());
        drop(owner);
        let restored = Engine::open(&path, Scenario::two_room(0))
            .expect("last journal owner must release its lock before returning");
        drop(inherited);
        // Closing an old descriptor must not release the replacement owner's lock.
        assert!(Engine::open(&path, Scenario::two_room(0)).is_err());
        drop(restored);
        Engine::open(&path, Scenario::two_room(0)).unwrap();
    }
}

impl Candidate {
    fn apply_wizard(
        &mut self,
        receipt: &Receipt,
        operation: &WizardOperation,
        entry_id: &EntryId,
        history: &[Record],
    ) -> Result<WizardResult, Failure> {
        let invalid = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Wizard target or settings are unavailable",
            )
        };
        let before: BTreeMap<_, _> = self
            .actors()
            .into_iter()
            .map(|actor| Ok((actor, self.revision_view(actor)?)))
            .collect::<Result<_, Failure>>()?;
        let result = match operation {
            WizardOperation::SetGravity { region, vector } => {
                self.game
                    .set_gravity(tor_world::RegionId(*region), *vector)
                    .map_err(|_| invalid())?;
                WizardResult::PhysicsSet
            }
            WizardOperation::SetCellGravity { position, vector } => {
                self.game
                    .set_cell_gravity(adapt::location(*position), *vector)
                    .map_err(|_| invalid())?;
                WizardResult::PhysicsSet
            }
            WizardOperation::SetBody {
                actor,
                cells,
                eye,
                mass,
            } => {
                self.game
                    .set_body(
                        SimActor(actor.0),
                        tor_simulation::BodySpec {
                            cells: cells.clone(),
                            eye: *eye,
                            mass: *mass,
                        },
                    )
                    .map_err(|_| invalid())?;
                WizardResult::PhysicsSet
            }
            WizardOperation::SetVelocity { actor, velocity } => {
                self.game
                    .set_actor_velocity(SimActor(actor.0), *velocity)
                    .map_err(|_| invalid())?;
                WizardResult::PhysicsSet
            }
            WizardOperation::ConnectPortal {
                from,
                direction,
                to,
                rotation,
                width,
                height,
            } => {
                self.game
                    .connect_portal_area(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::simulation_direction(*direction),
                            to: adapt::location(*to),
                        },
                        *rotation,
                        *width,
                        *height,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }

            WizardOperation::PlaceDoor {
                position,
                open,
                height,
            } => {
                let at = adapt::location(*position);
                let clearance = self.game.door_clearance(at);
                let door = self.game.place_door(at, *open, *height).map_err(|_| {
                    let message = if clearance > 0 && clearance < *height {
                        format!("Invalid door placement: at most {clearance} cells tall here")
                    } else {
                        "Invalid door placement".into()
                    };
                    Failure::new(ErrorCode::InvalidAction, &message)
                })?;
                WizardResult::DoorPlaced { door }
            }
            WizardOperation::ConnectArea {
                from,
                direction,
                to,
                quarter_turns,
                width,
                height,
            } => {
                self.game
                    .connect_area(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::simulation_direction(*direction),
                            to: adapt::location(*to),
                        },
                        *quarter_turns,
                        *width,
                        *height,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }
            WizardOperation::PlaceRoom { region } | WizardOperation::PlaceChamber { region } => {
                let chamber = matches!(operation, WizardOperation::PlaceChamber { .. });
                // Bound setup and disclosure work independently of wire limits.
                if region.id == 0
                    || region.name.is_empty()
                    || region.name.len() > 80
                    || region.name.chars().any(char::is_control)
                    || !(1..=32).contains(&region.width)
                    || !(1..=32).contains(&region.depth)
                    || !(1..=8).contains(&region.height)
                {
                    return Err(invalid());
                }
                let room = tor_world::Region {
                    id: tor_world::RegionId(region.id),
                    name: region.name.clone(),
                    bounds: tor_world::Extent::new(region.width, region.depth, region.height)
                        .ok_or_else(invalid)?,
                };
                if chamber {
                    self.game.add_chamber(room)
                } else {
                    self.game.add_region(room)
                }
                .map_err(|_| invalid())?;
                WizardResult::RoomPlaced { region: region.id }
            }
            WizardOperation::Connect {
                from,
                direction,
                to,
                quarter_turns,
            } => {
                self.game
                    .connect(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::simulation_direction(*direction),
                            to: adapt::location(*to),
                        },
                        *quarter_turns,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }
            WizardOperation::SetPlaceHint { position, present } => {
                self.game
                    .set_place_hint(adapt::location(*position), *present)
                    .map_err(|_| invalid())?;
                WizardResult::PlaceHintSet
            }
            WizardOperation::SetWall { position, wall } => {
                self.game
                    .set_wall(adapt::location(*position), *wall)
                    .map_err(|_| invalid())?;
                WizardResult::WallSet
            }
            WizardOperation::IdentifyItem { actor, item } => {
                self.game
                    .identify_item(SimActor(actor.0), tor_simulation::ItemId(*item))
                    .map_err(|_| invalid())?;
                WizardResult::ItemIdentified
            }
            WizardOperation::PlaceItem { kind, position } => {
                let name = match kind {
                    WizardItem::Token => "copper token",
                    WizardItem::Tablet => "stone tablet",
                };
                let item = self
                    .game
                    .place_item(adapt::location(*position), name.into())
                    .map_err(|_| invalid())?;
                WizardResult::ItemPlaced { item: item.0 }
            }
            WizardOperation::SpawnActor {
                position,
                turn_ticks,
            } => {
                let duration = NonZeroU64::new(*turn_ticks).ok_or_else(invalid)?;
                let actor = self
                    .game
                    .spawn_actor(adapt::location(*position), duration)
                    .map_err(|_| invalid())?;
                self.revisions.insert(ActorId(actor.0), 0);
                WizardResult::ActorSpawned {
                    actor: ActorId(actor.0),
                }
            }
            WizardOperation::Teleport { actor, position } => {
                self.game
                    .teleport(SimActor(actor.0), adapt::location(*position))
                    .map_err(|_| invalid())?;
                WizardResult::Teleported { actor: *actor }
            }
            WizardOperation::Rewind { target } => {
                if let Some(id) = target {
                    if !history.iter().any(|r| {
                        &r.entry.id == id && r.entry.visible_to(receipt.actor, &receipt.user)
                    }) {
                        return Err(invalid());
                    }
                }
                let boundary = self
                    .boundaries
                    .iter()
                    .find(|b| b.selectable && &b.id == target)
                    .cloned()
                    .ok_or_else(invalid)?;
                if boundary
                    .revisions
                    .any(receipt.actor, &boundary.game)
                    .is_none()
                {
                    return Err(invalid());
                }
                let later = std::mem::replace(&mut self.game, boundary.game.clone());
                // Retained futures can still refer to their entities and work;
                // new entities and work must never reuse those identities.
                self.game.continue_identities(&later);
                self.revisions = boundary.revisions.clone();
                let from_branch = self.current_branch.clone();
                self.current_branch = BranchId(entry_id.0.clone());
                return Ok(WizardResult::Rewound {
                    from_branch,
                    branch: self.current_branch.clone(),
                    tick: self.game.tick(),
                    next_actor: ActorId(self.game.next_actor().ok_or_else(invalid)?.0),
                });
            }
        };
        for (actor, old) in before {
            if actor == receipt.actor || self.revision_view(actor)? != old {
                let revision = self.revisions.get_mut(&actor).expect("existing actor");
                *revision = revision.checked_add(1).ok_or_else(invalid)?;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
thread_local! {
    static HISTORY_RECORD_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod scaling_tests {
    use super::*;

    #[test]
    fn history_page_work_does_not_scan_other_users_private_entries() {
        for hidden in [16, 256, 4096] {
            let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
            let mut append = |user: &str, request: &str, audience| {
                engine
                    .command(
                        user,
                        "history-index",
                        ActorId(1),
                        request,
                        &engine.branch().clone(),
                        Command::Annotate {
                            anchor: Anchor::State { revision: 0 },
                            category: AnnotationCategory::Note,
                            source: ClientSource::User,
                            audience,
                            text: request.into(),
                        },
                    )
                    .unwrap()
                    .entry
                    .id
            };
            let first = append("alice", "own-private", Audience::Private);
            let second = append("alice", "public", Audience::Actor);
            for index in 0..hidden {
                append("bob", &format!("hidden-{index}"), Audience::Private);
            }
            let before = HISTORY_RECORD_VISITS.with(|visits| visits.get());
            let page = engine.history(ActorId(1), "alice", None, 1).unwrap();
            assert_eq!(page.entries.len(), 1);
            assert_eq!(page.entries[0].id, second);
            assert_eq!(page.older_before, Some(second.clone()));
            assert_eq!(
                HISTORY_RECORD_VISITS.with(|visits| visits.get()) - before,
                1,
                "private entries: {hidden}"
            );
            let before = HISTORY_RECORD_VISITS.with(|visits| visits.get());
            let page = engine
                .history(ActorId(1), "alice", Some(&second), 1)
                .unwrap();
            assert_eq!(page.entries[0].id, first);
            assert_eq!(page.older_before, None);
            assert_eq!(
                HISTORY_RECORD_VISITS.with(|visits| visits.get()) - before,
                2
            );
        }
    }
    use tor_test_support::performance::Trace;

    fn checked_action(engine: &mut Engine, actor: ActorId, action: Action, request: usize) {
        let before: Vec<_> = engine
            .actors()
            .into_iter()
            .map(|id| {
                (
                    id,
                    engine.revision(id).unwrap(),
                    engine.revision_view(id).unwrap(),
                )
            })
            .collect();
        let mut reference = engine.game.clone();
        let result = engine.command(
            "oracle",
            "test",
            actor,
            &format!("action-{request}"),
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action: action.clone(),
            },
        );
        let expected = reference.act(SimActor(actor.0), adapt::action(&action));
        assert_eq!(result.is_ok(), expected.is_ok());
        if expected.is_ok() {
            reference.refresh_navigation();
        }
        assert_eq!(
            engine.game, reference,
            "optimized navigation must equal a full refresh"
        );
        for (id, revision, old) in before {
            let changed = engine.revision_view(id).unwrap() != old;
            assert_eq!(engine.revision(id).unwrap(), revision + u64::from(changed));
        }
    }

    #[test]
    fn optimized_decisions_match_full_views_and_navigation_for_every_actor() {
        let trace = Trace::load();
        for (regions, actors) in [(8, 1), (8, 8), (256, 8)] {
            let mut engine =
                Engine::memory(Scenario::performance(42, regions, actors).unwrap()).unwrap();
            let mut secondary = 0;
            let mut request = 0;
            for step in trace.steps(regions) {
                while engine.game.next_actor() != Some(SimActor(1)) {
                    let actor = ActorId(engine.game.next_actor().unwrap().0);
                    let action = if actor == ActorId(2) {
                        let action = trace.secondary[secondary % trace.secondary.len()]
                            .resolve(&engine.state(actor).unwrap());
                        secondary += 1;
                        engine.decode_action(actor, &action).unwrap()
                    } else {
                        Action::Wait
                    };
                    checked_action(&mut engine, actor, action, request);
                    request += 1;
                }
                let action = step.resolve(&engine.state(ActorId(1)).unwrap());
                let action = engine.decode_action(ActorId(1), &action).unwrap();
                checked_action(&mut engine, ActorId(1), action, request);
                request += 1;
            }
        }
    }

    #[test]
    fn rejected_transactions_preserve_receipts_history_boundaries_and_shared_state() {
        let directory = tempfile::tempdir().unwrap();
        let policy = crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(
            directory.path().join("rejected.db"),
            Scenario::two_room(42),
            policy,
        )
        .unwrap();
        let game = engine.game.clone();
        let branch = engine.branch().clone();
        let command = Command::Act {
            expected_revision: 0,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        for _ in 0..2 {
            assert_eq!(
                engine
                    .command(
                        "p",
                        "test",
                        ActorId(1),
                        "retryable",
                        &branch,
                        command.clone()
                    )
                    .unwrap_err()
                    .code,
                ErrorCode::StorageFailure
            );
            assert_eq!(engine.game, game);
            assert!(engine.receipts.is_empty() && engine.archive.records.is_empty());
            assert_eq!(engine.boundaries.len(), 1);
            assert_eq!(engine.revision(ActorId(1)).unwrap(), 0);
        }
    }
}
