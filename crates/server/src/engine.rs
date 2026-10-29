#[path = "survival_diagnostics.rs"]
mod survival_diagnostics;
#[cfg(test)]
use crate::journal::Direction;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tor_protocol::*;
use tor_simulation::floor_activation::FloorActivationRecord;
use tor_simulation::unique_retrieval::RetrievalEntry;
use tor_simulation::{ActorId as SimActor, CandidateStep, Game};
use uuid::Uuid;

use crate::adapt;
use crate::journal::{
    Action, Command, JournalContent, JournalEntry, Position, WizardItem, WizardOperation,
    WizardResult,
};

pub(crate) const ARCHIVE_VERSION: u32 = 64;
#[path = "action_resolution.rs"]
mod action_resolution;
#[path = "advancement_query.rs"]
mod advancement_query;
#[path = "arena_evaluation.rs"]
pub mod arena_evaluation;
#[path = "checkpoint.rs"]
mod checkpoint;
#[path = "command_request.rs"]
mod command_request;
use action_resolution::ActionResolution;
#[cfg(test)]
#[path = "event_history_boundary_tests.rs"]
mod event_history_boundary_tests;
#[path = "floor_activation_audit.rs"]
mod floor_activation_audit;
#[cfg(test)]
#[path = "floor_activation_tests.rs"]
mod floor_activation_tests;
#[path = "input_basis_generation.rs"]
mod input_basis_generation;
#[path = "intention.rs"]
mod intention;
use input_basis_generation::InputGenerations;
#[path = "intention_lifecycle.rs"]
mod intention_lifecycle;
#[cfg(test)]
#[path = "intention_tests.rs"]
mod intention_tests;
#[cfg(test)]
#[path = "interaction_tests.rs"]
mod interaction_tests;
#[cfg(test)]
#[path = "linked_hold_boundary_tests.rs"]
mod linked_hold_boundary_tests;
#[cfg(test)]
#[path = "map_memory_tests.rs"]
mod map_memory_tests;
#[cfg(test)]
#[path = "move_escape_admission_tests.rs"]
mod move_escape_admission_tests;
#[cfg(test)]
#[path = "package_invariant_tests.rs"]
mod package_invariant_tests;
#[cfg(test)]
#[path = "perception_persistence_tests.rs"]
mod perception_persistence_tests;
#[path = "projectile_choice_query.rs"]
mod projectile_choice_query;
#[cfg(test)]
#[path = "engine_region_count_tests.rs"]
mod region_count_tests;
pub(crate) use projectile_choice_query::PageBudget as ProjectileChoicePageBudget;
#[path = "wire_request.rs"]
mod wire_request;
pub(crate) use checkpoint::{Checkpoint, DiskCheckpoint};
const REWIND_BOUNDARIES: usize = 128;
const MAX_RETAINED_BOUNDARIES: usize = REWIND_BOUNDARIES * 2;
const RULESET: &str = crate::scenario_package::RULESET;

#[cfg(test)]
mod persisted_input_tests {
    use super::*;

    #[test]
    fn live_reads_prefer_later_flushed_snapshots_after_definition_cache_eviction() {
        for control in [
            "remove-artifact",
            "tamper-snapshot",
            "delete-before-first-read",
            "delete-known-snapshot",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("package");
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scenarios/tests/streaming-corridor");
            crate::scenario_package::streaming_corridor(&root, &source, 32, 5).unwrap();
            crate::scenario_package::validate(&source).unwrap();
            let scenario = crate::scenario_package::load(&source, 5, None, false).unwrap();
            let save = directory.path().join("game.db");
            let mut engine = Engine::open(&save, scenario).unwrap();
            let db = rusqlite::Connection::open(&save).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM region_sources WHERE region=5",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
                0
            );
            for step in 0..20 {
                engine
                    .command(
                        "test",
                        "test",
                        ActorId(1),
                        &format!("walk-{step}"),
                        &engine.branch().clone(),
                        Command::Act {
                            expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                            action: Action::Move {
                                direction: Direction::East,
                            },
                        },
                    )
                    .unwrap();
            }
            engine.flush().unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM region_sources WHERE region=5",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
                1
            );
            let package = engine.archive.scenario.package.as_ref().unwrap();
            // Reading a bounded run of distinct immutable definitions evicts5
            // without modifying geometry, queues, discoveries, or save rows.
            let evict = || {
                for region in 6..=32 {
                    package.region_def(region).unwrap();
                }
            };
            evict();
            if control == "remove-artifact" {
                std::fs::remove_dir_all(source.join(".compiled")).unwrap();
                assert_eq!(package.region_def(5).unwrap().name, "Hall 5");
            } else if control == "tamper-snapshot" {
                let text: String = db
                    .query_row(
                        "SELECT source FROM region_sources WHERE region=5",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                let mut input: serde_json::Value = serde_json::from_str(&text).unwrap();
                input["region"]["name"] = serde_json::Value::String("tampered after flush".into());
                db.execute(
                    "UPDATE region_sources SET source=?1 WHERE region=5",
                    [serde_json::to_string(&input).unwrap()],
                )
                .unwrap();
                let error = package.region_def(5).unwrap_err();
                assert!(
                    error
                        .message
                        .contains("Compiled region input hash mismatch"),
                    "{error}"
                );
            } else {
                if control == "delete-known-snapshot" {
                    package.region_def(5).unwrap();
                    evict();
                }
                assert_eq!(
                    db.execute("DELETE FROM region_sources WHERE region=5", [])
                        .unwrap(),
                    1
                );
                let error = package.region_def(5).unwrap_err();
                assert!(
                    error
                        .message
                        .contains("save's compiled region 5 is missing"),
                    "{error}"
                );
            }
        }
    }
}

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
                expected_boundary: engine.causal_boundary(actor).unwrap(),
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
            expected_boundary: 0,
            action: Action::Move {
                direction: Direction::East,
            },
        };
        let mut unauthorized = receipt.clone();
        unauthorized.request_id = "wizard".into();
        unauthorized.branch = BranchId("unavailable".into());
        unauthorized.command = Command::Wizard {
            display_basis: None,
            expected_boundary: 0,
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
        let expected_boundary = before.revision + 1;
        let commands = [
            Command::Act {
                expected_boundary,
                action: Action::Wait,
            },
            Command::Travel {
                display_basis: None,
                expected_boundary,
                destination: "unavailable".into(),
            },
            Command::RenamePlace {
                display_basis: None,
                expected_boundary,
                key: "unavailable".into(),
                name: "name".into(),
            },
            Command::Wizard {
                display_basis: None,
                expected_boundary,
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

    thread_local! {
        static READINESS_PROBE: std::cell::Cell<(usize, usize, usize)> = const { std::cell::Cell::new((0, 0, 0)) };
    }
    fn readiness_probe(name: &'static str, enter: bool) {
        READINESS_PROBE.with(|probe| {
            let (mut depth, mut views, mut scans) = probe.get();
            if name == "revision_view" {
                if enter {
                    depth += 1;
                    views += 1;
                } else {
                    depth -= 1;
                }
            } else if name == "scheduler" && enter && depth > 0 {
                scans += 1;
            }
            probe.set((depth, views, scans));
        });
    }

    #[test]
    fn remote_actor_boundaries_do_not_materialize_unrelated_observations() {
        // The small diagnostic constructor caps its built-in actor list at
        // eight. Reuse its validated locations across twenty distinct rooms.
        let mut scenario = Scenario::performance(42, 20, 5).unwrap();
        let local = scenario.actors.clone();
        scenario.actors = (1..=20)
            .flat_map(|region| {
                local.iter().cloned().map(move |mut actor| {
                    actor.position.region = region;
                    actor
                })
            })
            .collect();
        let mut engine = Engine::memory(scenario).unwrap();
        let actors = engine.actors();
        assert_eq!(actors.len(), 100);
        let before: Vec<_> = actors
            .iter()
            .map(|&actor| {
                (
                    actor,
                    engine.causal_boundary(actor).unwrap(),
                    engine.revision_view(actor).unwrap(),
                )
            })
            .collect();
        let count = tor_simulation::diagnostics::work_counts().observations;
        engine
            .command(
                "test",
                "test",
                ActorId(1),
                "change-driven-boundary",
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        let materialized = tor_simulation::diagnostics::work_counts().observations - count;
        assert_ne!(
            Some(
                engine
                    .revision_view(ActorId(1))
                    .unwrap()
                    .observation
                    .location
            ),
            Some(before[0].2.observation.location)
        );
        for (actor, boundary, _old) in before {
            assert_matches_uncached(&engine, actor);
            assert_eq!(
                engine.causal_boundary(actor).unwrap(),
                boundary + 1,
                "private causal position records every successful action, not visual differences"
            );
        }
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(replayed.game, engine.game);
        assert_eq!(
            serde_json::to_value(&replayed.archive.records).unwrap(),
            serde_json::to_value(&engine.archive.records).unwrap()
        );
        assert!(
            materialized < actors.len() / 2,
            "one actor move must not materialize all unrelated actor views: {materialized}"
        );
    }

    #[test]
    fn action_projection_reuses_settled_readiness_without_revising_disclosure() {
        let mut engine = Engine::memory(Scenario::performance(42, 8, 8).unwrap()).unwrap();
        let actors = engine.actors();
        assert_eq!(actors.len(), 8);
        assert_eq!(engine.game.next_actor(), Some(SimActor(1)));
        for &actor in &actors {
            engine.state(actor).unwrap();
        }
        let before = engine.state(ActorId(1)).unwrap();
        READINESS_PROBE.set((0, 0, 0));
        let observer = tor_simulation::diagnostics::spans::observe(readiness_probe);
        let (_, profile) = engine
            .command_profiled(
                "test",
                "test",
                ActorId(1),
                "readiness-context",
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        drop(observer);
        let (depth, views, scans) = READINESS_PROBE.get();
        assert_eq!(depth, 0);
        assert!(views < actors.len());
        assert!(profile.actors_observed < actors.len() * 2);
        assert_ne!(engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(
            engine.game.next_actor(),
            Some(SimActor(2)),
            "stable ready-time/identity tie"
        );
        for actor in actors {
            assert_matches_uncached(&engine, actor);
            assert_eq!(
                engine.revision_view(actor).unwrap().ready,
                actor == ActorId(2)
            );
        }
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(replayed.game, engine.game);
        assert_eq!(
            serde_json::to_value(&replayed.archive.records).unwrap(),
            serde_json::to_value(&engine.archive.records).unwrap()
        );
        for actor in engine.actors() {
            assert_eq!(replayed.state(actor).unwrap(), engine.state(actor).unwrap());
        }
        assert_eq!(
            scans, 0,
            "the settled after-boundary readiness must not be rescanned per observer"
        );
    }

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
    fn wizard_preloads_do_not_publish_views_from_before_mutation_or_rewind() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/streaming-corridor");
        let mut scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let mut engine = Engine::memory(scenario).unwrap();
        let actor = ActorId(1);
        engine.enable_wizard().unwrap();
        let initial = engine.state(actor).unwrap().observation;
        for (step, operation) in [
            WizardOperation::Teleport {
                actor,
                position: Position {
                    region: 7,
                    x: 4,
                    y: 1,
                    z: 0,
                },
            },
            WizardOperation::Rewind { target: None },
        ]
        .into_iter()
        .enumerate()
        {
            engine
                .command(
                    "test",
                    "test",
                    actor,
                    &format!("wizard-{step}"),
                    &engine.branch().clone(),
                    Command::Wizard {
                        display_basis: None,
                        expected_boundary: engine.causal_boundary(actor).unwrap(),
                        operation,
                    },
                )
                .unwrap();
            assert_matches_uncached(&engine, actor);
            if step == 0 {
                assert_ne!(engine.state(actor).unwrap().observation, initial);
            }
        }
        assert_eq!(engine.state(actor).unwrap().observation, initial);
    }

    #[test]
    fn streamed_move_views_survive_residency_changes_and_exact_replay() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/streaming-corridor");
        let mut scenario = crate::scenario_package::load(&root, 5, None, false).unwrap();
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let mut engine = Engine::memory(scenario).unwrap();
        let actor = ActorId(1);
        let mut transitions = 0;
        for step in 0..40 {
            for observer in engine.actors() {
                engine.state(observer).unwrap();
            }
            engine
                .command(
                    "player",
                    "test",
                    actor,
                    &format!("step-{step}"),
                    &engine.branch().clone(),
                    Command::AdmitIntention {
                        expected_boundary: engine.causal_boundary(actor).unwrap(),
                        action: Action::Move {
                            direction: Direction::East,
                        },
                    },
                )
                .unwrap();
            let mut profile = CommandProfile::default();
            let before = tor_simulation::diagnostics::work_counts();
            engine
                .execute_intention_profiled(None, Some(&mut profile))
                .unwrap()
                .unwrap();
            let after = tor_simulation::diagnostics::work_counts();
            transitions += profile.region_changes;
            assert!(
                after.observations - before.observations
                    <= engine.actors().len() * (1 + profile.region_changes),
                "residency comparison repeated a pre-transition observation: {profile:?}"
            );
            let retained = engine.observations.snapshot();
            let missing = engine
                .actors()
                .iter()
                .filter(|actor| !retained.contains_key(actor))
                .count();
            let before_read = tor_simulation::diagnostics::work_counts();
            let states: Vec<_> = engine
                .actors()
                .into_iter()
                .map(|observer| (observer, engine.state(observer).unwrap()))
                .collect();
            assert_eq!(
                tor_simulation::diagnostics::work_counts().observations,
                before_read.observations + missing,
                "reuse retained residency views; build each deferred view once at step {step}"
            );
            let after_read = tor_simulation::diagnostics::work_counts();
            for (observer, state) in states {
                assert_eq!(engine.state(observer).unwrap(), state);
            }
            assert_eq!(
                tor_simulation::diagnostics::work_counts().observations,
                after_read.observations
            );
        }
        assert!(transitions > 0);
        let expected = engine.state(actor).unwrap();
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(replayed.game, engine.game);
        assert_eq!(
            serde_json::to_value(&replayed.archive.records).unwrap(),
            serde_json::to_value(&engine.archive.records).unwrap()
        );
        assert_eq!(replayed.state(actor).unwrap(), expected);
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
                    display_basis: None,
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
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
                    display_basis: None,
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    operation: WizardOperation::Rewind { target: None },
                },
            )
            .unwrap();
        assert_matches_uncached(&engine, actor);
        let state = engine.state(actor).unwrap();
        let snapshot = SnapshotId("test-note".into());
        let receipt = Receipt {
            user: "test".into(),
            frontend: "test".into(),
            actor,
            request_id: "note".into(),
            branch: engine.branch().clone(),
            command: Command::Annotate {
                anchor: Anchor::State {
                    snapshot: snapshot.clone(),
                },
                text: "private note".into(),
                source: ClientSource::User,
                audience: Audience::Private,
                category: AnnotationCategory::Note,
            },
        };
        let anchor = crate::journal::RecordedAnchor::State {
            snapshot,
            boundary: engine.presentation_boundary().entry().cloned(),
        };
        engine.command_with_annotation(&receipt, anchor).unwrap();
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
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        let missing = usize::from(!engine.observations.snapshot().contains_key(&actor));
        let before = tor_simulation::diagnostics::work_counts();
        let moved = engine.state(actor).unwrap();
        assert_ne!(moved, initial);
        assert_eq!(engine.state(actor).unwrap(), moved);
        assert_eq!(
            tor_simulation::diagnostics::work_counts().observations,
            before.observations + missing,
            "reuse a retained post-action view or materialize its deferred replacement once"
        );
        engine
            .command(
                "test",
                "test",
                actor,
                "wait",
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
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
                            expected_boundary: normal.causal_boundary(actor).unwrap(),
                            action: Action::Wait,
                        },
                    )
                    .unwrap();
            }
            assert_eq!(seeded.game, normal.game);
            assert_eq!(seeded.game, normal.game);
            assert_eq!(seeded.boundaries.len(), normal.boundaries.len());
            for (a, b) in seeded.boundaries.iter().zip(&normal.boundaries) {
                assert_eq!(a.game, b.game);
                assert_eq!(a.game, b.game);
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
    pub(crate) fn simulation_failure(error: tor_simulation::GameError) -> Option<Self> {
        if matches!(error, tor_simulation::GameError::PopulationCapacity(_)) {
            return Some(Self::new(
                ErrorCode::ResourceLimit,
                "Floor entry exceeds current simulation capacity; no movement or population was committed",
            ));
        }
        use tor_simulation::creatures::{BuildError, CreatureStateError};
        use tor_simulation::GameError;
        let capacity = matches!(
            error,
            GameError::IdentityExhausted
                | GameError::Build(
                    BuildError::Arithmetic | BuildError::Allocation | BuildError::TooManyHitDice
                )
                | GameError::Creature(
                    CreatureStateError::BuildEpochExhausted
                        | CreatureStateError::Build(
                            BuildError::Arithmetic
                                | BuildError::Allocation
                                | BuildError::TooManyHitDice
                        )
                        | CreatureStateError::Growth(
                            tor_simulation::earned_growth::GrowthError::Arithmetic
                        )
                )
                | GameError::Growth(
                    CreatureStateError::BuildEpochExhausted
                        | CreatureStateError::Build(
                            BuildError::Arithmetic
                                | BuildError::Allocation
                                | BuildError::TooManyHitDice
                        )
                        | CreatureStateError::Growth(
                            tor_simulation::earned_growth::GrowthError::Arithmetic
                        )
                )
        );
        if capacity {
            return Some(Self::new(
                ErrorCode::ResourceLimit,
                "Simulation resource capacity exhausted",
            ));
        }
        matches!(error, GameError::Growth(_)).then(|| {
            Self::new(
                ErrorCode::InvalidAction,
                "Simulation state could not be updated; no changes were committed",
            )
        })
    }

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
    pub(crate) fn ruleset_hash(&self) -> String {
        self.package.as_ref().map_or_else(
            || {
                crate::scenario_package::source_digest(include_str!(
                    "../../simulation/data/default-ruleset.toml"
                ))
            },
            |package| package.certificate.ruleset_hash.clone(),
        )
    }
    pub(crate) fn content(&self) -> Result<Arc<tor_simulation::creatures::Content>, Failure> {
        match &self.package {
            Some(package) => package.content(),
            None => diagnostic_content(),
        }
    }
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

pub(crate) fn diagnostic_content() -> Result<Arc<tor_simulation::creatures::Content>, Failure> {
    let document = toml::from_str(include_str!("../../simulation/data/default-ruleset.toml"))
        .map_err(|_| invalid_archive())?;
    let rules =
        tor_simulation::rules::Ruleset::validate(document).map_err(|_| invalid_archive())?;
    tor_simulation::creatures::Content::new(rules, Default::default())
        .map_err(|_| invalid_archive())
}

fn scenario_game(scenario: &Scenario) -> Result<Game, Failure> {
    let content = scenario.content()?;
    if scenario.regions <= 2 {
        return Ok(Game::two_room_in_stone(scenario.seed, content));
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
    Ok(Game::new(world, scenario.seed, content))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    pub(crate) user: String,
    pub(crate) frontend: String,
    pub(crate) request_id: String,
    #[serde(with = "crate::storage::schema::ActorId")]
    pub(crate) actor: ActorId,
    #[serde(with = "crate::storage::schema::BranchId")]
    pub(crate) branch: BranchId,
    pub(crate) command: Command,
}

/// Small independent evidence survives expiration of a branch's snapshots.
/// It carries counters and the bound ordinary ID, never another location store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RetrievalBoundaryState {
    pub(crate) initialized: bool,
    pub(crate) item: Option<tor_simulation::ItemId>,
    pub(crate) entry_count: u64,
    pub(crate) placement_count: u64,
}

impl<'de> Deserialize<'de> for RetrievalBoundaryState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            initialized: bool,
            #[serde(deserialize_with = "required_retrieval_item")]
            item: Option<tor_simulation::ItemId>,
            entry_count: u64,
            placement_count: u64,
        }
        let fields: Fields = crate::storage::schema::named_map(d)?;
        Ok(Self {
            initialized: fields.initialized,
            item: fields.item,
            entry_count: fields.entry_count,
            placement_count: fields.placement_count,
        })
    }
}

/// The one bounded receipt fold used by live admission and historical audit.
/// Locations remain the ordinary item store's authority. This only corroborates
/// this transaction's identity/counter changes, never scans retained history.
fn fold_retrieval_entries(
    mut state: Option<RetrievalBoundaryState>,
    entries: &[RetrievalEntry],
) -> Result<Option<RetrievalBoundaryState>, Failure> {
    use tor_simulation::unique_retrieval::RetrievalResult;
    if entries.len() > tor_simulation::unique_retrieval::MAX_RETRIEVAL_ENTRIES {
        return Err(invalid_archive());
    }
    let Some(summary) = &mut state else {
        return if entries.is_empty() {
            Ok(None)
        } else {
            Err(invalid_archive())
        };
    };
    if !summary.initialized
        || summary.placement_count > summary.entry_count
        || match summary.item {
            None => summary.entry_count != 0 || summary.placement_count != 0,
            Some(item) => item.0 == 0 || summary.entry_count == 0 || summary.placement_count == 0,
        }
    {
        return Err(invalid_archive());
    }
    for entry in entries {
        let ordinal = summary
            .entry_count
            .checked_add(1)
            .ok_or_else(invalid_archive)?;
        if entry.ordinal != ordinal {
            return Err(invalid_archive());
        }
        match entry.result {
            RetrievalResult::Created {
                item, placement, ..
            } => {
                if summary.item.is_some() || item.0 == 0 || ordinal != 1 || placement != 1 {
                    return Err(invalid_archive());
                }
                summary.item = Some(item);
                summary.placement_count = 1;
            }
            RetrievalResult::Relocated {
                item, placement, ..
            } => {
                if summary.item != Some(item)
                    || summary.placement_count.checked_add(1) != Some(placement)
                {
                    return Err(invalid_archive());
                }
                summary.placement_count = placement;
            }
            RetrievalResult::AlreadyCarried { item } => {
                if summary.item != Some(item) {
                    return Err(invalid_archive());
                }
            }
        }
        summary.entry_count = ordinal;
    }
    Ok(state)
}

impl RetrievalBoundaryState {
    fn capture(game: &Game) -> Option<Self> {
        game.unique_retrieval_state().map(|state| Self {
            initialized: state.initialized,
            item: state.item,
            entry_count: state.entry_count,
            placement_count: state.placement_count,
        })
    }
}

fn required_retrieval_item<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<tor_simulation::ItemId>, D::Error> {
    Option::<tor_simulation::ItemId>::deserialize(d)
}
fn required_retrieval_boundary<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<RetrievalBoundaryState>, D::Error> {
    Option::<RetrievalBoundaryState>::deserialize(d)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    /// Required private transaction clocks; wizard public ticks describe results.
    pub(crate) before_tick: u64,
    pub(crate) after_tick: u64,
    #[serde(deserialize_with = "required_retrieval_boundary")]
    pub(crate) retrieval_before: Option<RetrievalBoundaryState>,
    #[serde(deserialize_with = "required_retrieval_boundary")]
    pub(crate) retrieval_after: Option<RetrievalBoundaryState>,
    pub(crate) entry: JournalEntry,
    pub(crate) receipt: Option<Receipt>,
    /// Required private binding between the original input and admitted work.
    pub(crate) action_resolution: ActionResolution,
    /// Exact new ledger entries, private to persistence and replay.
    pub(crate) activations: Vec<FloorActivationRecord>,
    /// Latest owning transaction only; empty non-entry deltas remain explicit.
    #[serde(deserialize_with = "read_retrieval_entries")]
    pub(crate) retrievals: Vec<RetrievalEntry>,
}

fn read_retrieval_entries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RetrievalEntry>, D::Error> {
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = Vec<RetrievalEntry>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded retrieval entry batch")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            let limit = tor_simulation::unique_retrieval::MAX_RETRIEVAL_ENTRIES;
            if sequence.size_hint().is_some_and(|size| size > limit) {
                return Err(A::Error::custom("retrieval entry batch exceeds limit"));
            }
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == limit {
                    return Err(A::Error::custom("retrieval entry batch exceeds limit"));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries)
}

impl Record {
    pub(crate) fn named_spawn_receipt_matches(&self, operation: &WizardOperation) -> bool {
        self.receipt.as_ref().is_some_and(|receipt| {
            matches!(&receipt.command,
                Command::Wizard { operation: requested, .. } if requested == operation)
        })
    }

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
                                step: None,
                                ..
                            },
                            IntentionChange::Cancelled,
                        ) => requested == admission,
                        (
                            Command::CancelIntention {
                                admission: requested,
                                step: Some(step),
                                ..
                            },
                            IntentionChange::Escaped { direction },
                        ) => requested == admission && step == direction,
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
                } => {
                    original_action.is_some_and(|action| executed == action && action.is_prepared())
                }
                JournalContent::IntentionFailed { .. } => true,
                JournalContent::IntentionContinuationFailed { .. } => {
                    original_action.is_some_and(|action| action.is_prepared())
                }
                _ => false,
            }
    }

    /// Check audit metadata even when recovery restores a checkpoint instead of replaying.
    pub(crate) fn valid_admission(&self) -> bool {
        if !self.valid_action_resolution() {
            return false;
        }
        if self.receipt.as_ref().is_some_and(|receipt| {
            matches!(
                receipt.command,
                Command::ResumeIntention { .. } | Command::CancelIntention { .. }
            )
        }) && self.change_source().is_none()
        {
            return false;
        }
        if let JournalContent::Annotation { anchor, .. } = &self.entry.content {
            if let Some(receipt) = &self.receipt {
                if !matches!(&receipt.command, Command::Annotate { anchor: raw, .. } if *raw == anchor.to_public())
                {
                    return false;
                }
            }
        } else if self
            .receipt
            .as_ref()
            .is_some_and(|receipt| matches!(receipt.command, Command::Annotate { .. }))
        {
            return false;
        }
        match (&self.entry.content, self.receipt.as_ref()) {
            (
                JournalContent::IntentionAdmitted {
                    intention,
                    action,
                    human_operands,
                },
                Some(receipt),
            ) => {
                intention.0 != 0
                    && human_operands.structurally_valid_for(&adapt::action(action))
                    && matches!(&receipt.command, Command::AdmitIntention { .. })
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
    pub(crate) ruleset_hash: String,
    #[serde(with = "crate::storage::schema::scenario")]
    pub(crate) scenario: Scenario,
    #[serde(with = "crate::storage::schema::BranchId")]
    pub(crate) branch: BranchId,
    pub(crate) records: Vec<Record>,
    /// The admitted starting floor belongs to the base, not a fake action.
    pub(crate) initial_activations: Vec<FloorActivationRecord>,
    #[serde(deserialize_with = "read_retrieval_entries")]
    pub(crate) initial_retrievals: Vec<RetrievalEntry>,
}

/// How many of a streaming game's regions are in each state, and how its
/// region records are kept. Includes all authored IDs and declared runtime IDs.
/// Unvisited procedural addresses are not counted until declared or generated;
/// the potential size or operational cap of a family never inflates these counts.
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
    pub intention_derivation: Duration,
    pub request_validation: Duration,
    pub map_recording: Duration,
    pub commit_metadata: Duration,
    pub input_generation: Duration,
    pub intention_actor_visits: usize,
    pub intention_candidate_visits: usize,
    pub intention_event_visits: usize,
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
    /// Map acquisition consumes appearance facts, reusing a rich view if present.
    pub map_rich_views_reused: usize,
    pub map_appearance_projections: usize,
    pub previous_observer_scene_reads: usize,
    pub revision_comparisons: usize,
    /// Surviving action-selected observers, including stationary capability changes.
    pub action_observer_boundaries: usize,
    pub action_event_receipts_recorded: usize,
    pub action_projection_validated: usize,
    pub action_navigation_reviewed: usize,
    /// Dependency proofs, separate from cold full-view comparisons.
    pub observation_changes_proven: usize,
    pub observations_unchanged_proven: usize,
    pub observation_projection_fallbacks: usize,
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
            + self.intention_derivation
            + self.request_validation
            + self.map_recording
            + self.commit_metadata
            + self.input_generation
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

#[cfg(test)]
mod profile_accounting_tests {
    use super::*;

    #[test]
    fn narrow_boundary_phases_are_counted_once_in_exclusive_duration() {
        let profile = CommandProfile {
            intention_derivation: Duration::from_nanos(1),
            request_validation: Duration::from_nanos(2),
            map_recording: Duration::from_nanos(4),
            commit_metadata: Duration::from_nanos(8),
            input_generation: Duration::from_nanos(16),
            ..CommandProfile::default()
        };
        assert_eq!(profile.exclusive_duration(), Duration::from_nanos(31));
    }
}

#[derive(Clone, Debug)]
struct Boundary {
    id: Option<EntryId>,
    /// Derived from journal content, never trusted from a checkpoint payload.
    selectable: bool,
    game: Game,
}

/// Opaque immutable rules boundary retained for issued presentation capabilities.
/// Clones share existing rewind ownership; they never copy a Game or grant access.
#[derive(Clone, Debug)]
pub struct PresentationBoundary {
    boundary: Arc<Boundary>,
    branch: BranchId,
    owner: String,
    wizard_game: bool,
}

impl PresentationBoundary {
    pub fn branch(&self) -> &BranchId {
        &self.branch
    }
    pub fn entry(&self) -> Option<&EntryId> {
        self.boundary.id.as_ref()
    }
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

    observations: BTreeMap<ActorId, Arc<CachedObservation>>,
    /// Region records this command's transition made.
    made: Vec<(
        tor_simulation::RecordId,
        tor_world::Shared<tor_simulation::RegionRecord>,
    )>,
}

impl Candidate {
    /// Cold full-projection oracle retained only for causal equivalence tests.
    #[cfg(test)]
    fn revision_view(&self, actor: ActorId) -> Result<CachedObservation, Failure> {
        revision_view(&self.game, actor)
    }

    fn capture(engine: &Engine) -> Self {
        let mut game = engine.game.clone();
        game.begin_survival_diagnostics(engine.survival_log.is_some());
        Self {
            current_branch: engine.current_branch.clone(),
            boundaries: engine.boundaries.clone(),
            game,

            observations: BTreeMap::new(),
            made: Vec::new(),
        }
    }
    fn branch(&self) -> &BranchId {
        &self.current_branch
    }
    /// Actors in loaded regions.
    fn actors(&self) -> Vec<ActorId> {
        self.game
            .loaded_actor_ids()
            .map(|id| ActorId(id.0))
            .collect()
    }
    /// Population can create ordinary actors without changing residency. Keep
    /// their navigation and observer knowledge inside the transaction.
    fn reconcile_actors(
        &mut self,
        before: &Game,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        let added: Vec<_> = self
            .game
            .loaded_actor_ids()
            .map(|id| ActorId(id.0))
            .filter(|id| !before.has_actor(SimActor(id.0)))
            .collect();
        self.observations
            .retain(|actor, _| self.game.has_actor(SimActor(actor.0)));
        if !added.is_empty() {
            if let Some(profile) = profile.as_deref_mut() {
                profile.navigation_refreshes += 1;
            }
        }
        for actor in added {
            let started = Instant::now();
            self.game
                .validate_observer_projection(SimActor(actor.0))
                .map_err(|_| invalid_archive())?;
            let scene = self
                .game
                .capture_observation_scene_shared(SimActor(actor.0))
                .map_err(|_| invalid_archive())?;
            if let Some(profile) = profile.as_deref_mut() {
                profile.perception += started.elapsed();
            }
            let started = Instant::now();
            self.game
                .refresh_navigation_scene(SimActor(actor.0), &scene);
            if let Some(profile) = profile.as_deref_mut() {
                profile.navigation_refresh += started.elapsed();
            }
        }
        Ok(())
    }

    /// Learn only from the final candidate's ordinary observer projections.
    /// Queries stay pure, and storage rejection drops this candidate wholesale.
    fn record_explored_knowledge(
        &mut self,
        before: &Game,
        terrain: adapt::Terrain<'_>,
        profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        let changed = self.game.map_disclosure_changed_actors_since(
            before,
            self.actors().into_iter().map(|actor| SimActor(actor.0)),
        );
        self.record_explored_for_actors(changed, terrain, profile)
    }

    fn record_explored_for_actors(
        &mut self,
        actors: Vec<SimActor>,
        terrain: adapt::Terrain<'_>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        let _span = tor_simulation::diagnostics::spans::span("map_acquisition");
        for actor in actors {
            let actor = ActorId(actor.0);
            let (inputs, scene) = match self.observations.get(&actor) {
                Some(view) => {
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.map_rich_views_reused += 1;
                    }
                    (
                        crate::map_memory::inputs(&view.observation, terrain),
                        view.scene.clone(),
                    )
                }
                None => {
                    let started = Instant::now();
                    let (view, scene) = {
                        let _span =
                            tor_simulation::diagnostics::spans::span("map_acquire_projection");
                        self.game
                            .observe_map_shared_scene(SimActor(actor.0))
                            .map_err(|_| invalid_archive())?
                    };
                    let inputs = crate::map_memory::appearance_inputs(
                        &view.visible_cells,
                        &view.exits,
                        &view.ground_items,
                        terrain,
                    );
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.perception += started.elapsed();
                        profile.map_appearance_projections += 1;
                    }
                    (inputs, scene)
                }
            };
            let _span = tor_simulation::diagnostics::spans::span("map_record_explored");
            let started = profile.as_ref().map(|_| Instant::now());
            self.game
                .record_explored_observation(SimActor(actor.0), &inputs, &scene)
                .map_err(|_| invalid_archive())?;
            if let Some(profile) = profile.as_deref_mut() {
                profile.map_recording += started.expect("profile clock").elapsed();
            }
        }
        Ok(())
    }

    fn record_observation_knowledge(
        &mut self,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        // Eligibility comes from this semantic knowledge pass, not free-read
        // cache warmth. The ordinary action revision pass has captured its own
        // observers already, including known-ability and death-only events.
        let witness = self.game.event_observation_witness();
        let candidates = self
            .game
            .observation_knowledge_candidates()
            .map_err(|_| invalid_archive())?;
        if !candidates.is_empty() {
            for actor in self.actors() {
                if !self
                    .game
                    .may_add_observation_knowledge(SimActor(actor.0), &candidates)
                {
                    continue;
                }
                let started = Instant::now();
                let facts = self
                    .game
                    .capture_commit_event_facts(SimActor(actor.0))
                    .map_err(|_| invalid_archive())?;
                if self.game.event_observation_pending(facts.proof()) {
                    if let Some(view) = self.observations.get_mut(&actor) {
                        // Preserve an already captured map/presentation view;
                        // only append historical authority licensed right now.
                        let view = Arc::make_mut(view);
                        self.game
                            .record_event_observation(
                                &witness,
                                facts.proof(),
                                &mut view.observation,
                            )
                            .map_err(|_| invalid_archive())?;
                    } else {
                        self.game
                            .record_commit_event_facts(&witness, &facts)
                            .map_err(|_| invalid_archive())?;
                    }
                }
                self.game
                    .record_committed_event_knowledge(SimActor(actor.0))
                    .map_err(|_| invalid_archive())?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.perception += started.elapsed();
                }
            }
        }
        self.game
            .seal_event_observations()
            .map_err(|_| invalid_archive())?;
        Ok(())
    }
    /// Publish, then ask the preloader for what the next transitions are
    /// likely to need.
    fn publish(self, engine: &mut Engine) -> Option<crate::regions::PreloadWork> {
        engine.current_branch = self.current_branch;
        engine.boundaries = self.boundaries;
        engine.game = self.game;
        engine
            .game
            .set_combat_diagnostics(engine.combat_diagnostics_enabled);
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
            | WizardOperation::SpawnActor { position, .. }
            | WizardOperation::SpawnCreature { position, .. } => vec![Some(at(position))],
            WizardOperation::ConnectPortal { from, to, .. }
            | WizardOperation::ConnectArea { from, to, .. }
            | WizardOperation::Connect { from, to, .. } => vec![Some(at(from)), Some(at(to))],
            WizardOperation::Teleport {
                actor: id,
                position,
            } => vec![actor(id), Some(at(position))],
            WizardOperation::AdvanceCreature { actor: id, .. }
            | WizardOperation::SetCreatureTemplate { actor: id, .. }
            | WizardOperation::RemoveCreatureHitDie { actor: id }
            | WizardOperation::SetBody { actor: id, .. }
            | WizardOperation::SetVelocity { actor: id, .. }
            | WizardOperation::IdentifyItem { actor: id, .. } => vec![actor(id)],
            WizardOperation::PlaceChamber { .. }
            | WizardOperation::PlaceRoom { .. }
            | WizardOperation::Rewind { .. }
            | WizardOperation::ArenaControl { .. } => vec![],
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
        self.transition_with_observations(regions, extra, profile, true)
    }

    /// Finish a provisional simulation handoff before taking any observer view.
    /// The action caller compares the original committed and final states once.
    fn settle_action_entry(
        &mut self,
        regions: Option<&mut crate::regions::Regions>,
        profile: Option<&mut CommandProfile>,
    ) -> Result<(), Failure> {
        if !self.game.region_transition_pending() {
            return Ok(());
        }
        self.transition_with_observations(regions, BTreeSet::new(), profile, false)?;
        if self.game.region_transition_pending() {
            return Err(invalid_archive());
        }
        Ok(())
    }

    fn transition_with_observations(
        &mut self,
        regions: Option<&mut crate::regions::Regions>,
        extra: std::collections::BTreeSet<tor_world::RegionId>,
        mut profile: Option<&mut CommandProfile>,
        compare: bool,
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
        if let Some(profile) = profile.as_deref_mut() {
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
        if !compare {
            return Ok(());
        }
        if let Some(before) = work.before {
            // Loading, detaching, or activating regions can change both the
            // projected scene and scheduler readiness. Recompute at the new
            // boundary instead of publishing a pre-transition observation.
            self.observations.clear();
            for actor in before
                .loaded_actor_ids()
                .filter(|id| self.game.has_actor(*id))
            {
                before
                    .validate_observer_projection(actor)
                    .map_err(|_| invalid_archive())?;
                self.game
                    .validate_observer_projection(actor)
                    .map_err(|_| invalid_archive())?;
            }
            self.reconcile_actors(&before, profile)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedObservation {
    observation: tor_simulation::Observation,
    charts: Arc<crate::chart_view::PreparedCharts>,
    scene: Arc<Vec<tor_world::SightCell>>,
    ready: bool,
}

#[cfg(test)]
thread_local! {
    static FULL_PICTURE_ADAPTATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One event boundary's authorized narration inputs and immutable source. This
/// is transient Service ownership, not a persisted presentation cache.
#[derive(Clone, Debug)]
pub(crate) struct SemanticObservation {
    pub(crate) facts: tor_protocol::OutcomeObservation,
    actor: ActorId,
    boundary: PresentationBoundary,
    view: Arc<CachedObservation>,
}

/// Derived views of the committed boundary, never encoded into a save or a
/// rewind snapshot. The mutex preserves Engine's thread-safe read interface.
#[derive(Debug, Default)]
struct ObservationCache(Mutex<BTreeMap<ActorId, Arc<CachedObservation>>>);

impl ObservationCache {
    fn view(&self, game: &Game, actor: ActorId) -> Result<Arc<CachedObservation>, Failure> {
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

    fn snapshot(&self) -> BTreeMap<ActorId, Arc<CachedObservation>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn replace(&mut self, views: BTreeMap<ActorId, Arc<CachedObservation>>) {
        *self
            .0
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = views;
    }
}

fn revision_view(game: &Game, actor: ActorId) -> Result<CachedObservation, Failure> {
    let _diagnostic_span = tor_simulation::diagnostics::spans::span("revision_view");
    let (view, scene, _) = game
        .observe_event_shared_scene(SimActor(actor.0))
        .map_err(|_| invalid_archive())?;
    Ok(CachedObservation {
        charts: Arc::new(crate::chart_view::PreparedCharts::for_game(
            game, actor, false,
        )?),
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
    /// Runtime tracing policy; independent of rewind/save state.
    combat_diagnostics_enabled: bool,
    survival_log: Option<survival_diagnostics::SurvivalLog>,
    boundaries: VecDeque<Arc<Boundary>>,
    game: Game,
    archive: Archive,

    observations: ObservationCache,
    input_generations: InputGenerations,
    history_index: crate::history_index::HistoryIndex,
    receipts: BTreeMap<(String, String), usize>,
    path: Option<PathBuf>,
    lock: Option<Arc<crate::storage::JournalLock>>,
    store: Option<crate::storage::Store>,
    /// Region records, for games that stream.
    regions: Option<crate::regions::Regions>,
}

#[cfg(test)]
pub(crate) struct DetachedChartTestFixture {
    pub(crate) game: Game,
    pub(crate) records: Vec<(
        tor_simulation::RecordId,
        tor_world::Shared<tor_simulation::RegionRecord>,
    )>,
    pub(crate) package: Arc<crate::scenario_package::Package>,
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
    pub(crate) fn arena_execution_enabled(&self) -> bool {
        self.game.arena_execution_enabled()
    }

    pub(crate) fn unattended_arena(&self) -> bool {
        self.game.arena_run().is_some()
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
                None => scenario_game(&scenario)?,
            },
            (Some(1), None) => crate::performance_fixture::game(scenario.seed, scenario.regions)?,
            _ => return Err(invalid_archive()),
        };
        if scenario.actors.is_empty() && scenario.package.is_none() {
            return Err(invalid_archive());
        }
        for actor in &scenario.actors {
            let ticks = NonZeroU64::new(actor.turn_ticks).ok_or_else(invalid_archive)?;
            game.spawn_actor(adapt::location(actor.position), ticks)
                .map_err(|_| invalid_archive())?;
        }
        game.initialize_discovery().map_err(|_| invalid_archive())?;
        game.refresh_navigation();
        // Initial knowledge is part of the initial immutable boundary, never a
        // side effect of the first client query.
        let terrain = |region: tor_world::RegionId| {
            scenario
                .package
                .as_deref()
                .and_then(|p| p.region_terrain(region.0))
                .map_or([None, None, None], |t| {
                    [t.floor.clone(), t.wall.clone(), t.door.clone()]
                })
        };
        let mut initial_views = BTreeMap::new();
        for actor in game
            .loaded_actor_ids()
            .map(|id| ActorId(id.0))
            .collect::<Vec<_>>()
        {
            let view = Arc::new(revision_view(&game, actor)?);
            game.record_explored_observation(
                SimActor(actor.0),
                &crate::map_memory::inputs(&view.observation, &terrain),
                &view.scene,
            )
            .map_err(|_| invalid_archive())?;
            initial_views.insert(actor, view);
        }
        let initial_activations = game.activations_since(0).map_err(|_| invalid_archive())?;
        let initial_retrievals = game.retrieval_entries().to_vec();
        let branch = BranchId(Uuid::new_v4().to_string());
        let initial = Arc::new(Boundary {
            id: None,
            selectable: true,
            game: game.clone(),
        });
        Ok(Self {
            recovery: RecoveryProfile::default(),
            current_branch: branch.clone(),
            wizard_enabled: false,
            combat_diagnostics_enabled: false,
            survival_log: None,
            boundaries: VecDeque::from([initial]),
            game,
            receipts: BTreeMap::new(),
            observations: ObservationCache(Mutex::new(initial_views)),
            input_generations: InputGenerations::default(),
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
                ruleset_hash: scenario.ruleset_hash(),
                scenario,
                branch,
                records: Vec::new(),
                initial_activations,
                initial_retrievals,
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
        crate::process_diagnostics::startup("restore.lock.begin");
        let (path, lock) = crate::storage::JournalLock::acquire(path.as_ref())?;
        crate::process_diagnostics::startup("restore.lock.end");
        crate::process_diagnostics::startup("restore.store.begin");
        let supplied = scenario.package.clone();
        let (store, mut archive, checkpoint, saved) = crate::storage::Store::open(
            &path,
            || {
                let engine = Self::memory(scenario)?;
                let sources = engine.unsaved_sources()?;
                let records = engine
                    .regions
                    .as_ref()
                    .map(|regions| regions.encoded_resident())
                    .transpose()?
                    .unwrap_or_default();
                Ok((engine.archive, sources, records))
            },
            policy,
            lock.clone(),
        )?;
        crate::process_diagnostics::startup("restore.store.end");
        // New and resumed games use the same frozen snapshots and artifact.
        // Authored paths remain available only for diagnostic source spans.
        if let Some(package) = &mut archive.scenario.package {
            let package = Arc::make_mut(package);
            package.sources = package.sources.independent();
            package.attach_persisted_inputs(store.source_reader(), supplied.as_deref());
        }
        let records_loaded = archive.records.len();
        let checkpoint_sequence = checkpoint.as_ref().map(|c| c.sequence).unwrap_or(0);
        let records_replayed = records_loaded
            - checkpoint
                .as_ref()
                .map(|c| c.record_count)
                .unwrap_or(0)
                .min(records_loaded);
        crate::process_diagnostics::startup("restore.replay.begin");
        let mut engine = Self::replay(archive, checkpoint, Some(&store))?;
        crate::process_diagnostics::startup("restore.replay.end");
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
        if let Some(store) = &self.store {
            store.flush()?;
        }
        self.survival_save_barrier(self.save_status().durable_sequence);
        Ok(())
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
        // Authored IDs preserve the finite catalog's unbuilt count. Runtime
        // lifecycle IDs also include generated families absent from that index.
        // Deduplicate overlap without enumerating any procedural address space.
        let known: BTreeSet<_> = package
            .index
            .regions
            .iter()
            .map(|region| tor_world::RegionId(region.id))
            .chain(self.game.loaded_regions())
            .chain(self.game.unbuilt_regions())
            .chain(self.game.detached_records().map(|(region, _)| region))
            .collect();
        for region in known {
            match self.game.region_state(region) {
                Some(tor_simulation::RegionState::Active) => counts.active += 1,
                Some(tor_simulation::RegionState::Frozen) => counts.frozen += 1,
                Some(tor_simulation::RegionState::Detached) => counts.detached += 1,
                // Authored regions may not yet be declared in the game.
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
            || archive.ruleset_hash != archive.scenario.ruleset_hash()
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
            if engine.archive.initial_activations != archive.initial_activations
                || engine.archive.initial_retrievals != archive.initial_retrievals
            {
                return Err(invalid_archive());
            }
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
            if record.before_tick != engine.game.tick()
                || record.retrieval_before != RetrievalBoundaryState::capture(&engine.game)
                || !record.valid_admission()
                || Uuid::parse_str(&record.entry.id.0).is_err()
                || engine.history_index.find(&record.entry.id).is_some()
            {
                return Err(invalid_archive());
            }
            let recorded_annotation = match &record.entry.content {
                JournalContent::Annotation { anchor, .. } => Some(anchor.clone()),
                _ => None,
            };
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
                        .apply_command_with_annotation(
                            &receipt,
                            Some(record.entry.id.clone()),
                            recorded_annotation.clone(),
                        )
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
                    .apply_command_with_annotation(
                        &receipt,
                        Some(record.entry.id.clone()),
                        recorded_annotation.clone(),
                    )
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
            if result.map_err(|_| invalid_archive())? != record.entry
                || engine.archive.records.last().is_none_or(|actual| {
                    actual.action_resolution != record.action_resolution
                        || actual.activations != record.activations
                        || actual.retrievals != record.retrievals
                        || actual.retrieval_before != record.retrieval_before
                        || actual.retrieval_after != record.retrieval_after
                        || actual.before_tick != record.before_tick
                        || actual.after_tick != record.after_tick
                })
            {
                return Err(invalid_archive());
            }
        }
        // Detached blobs remain lazy. Their historical chart source authority
        // is checked by Regions at actual materialization, before core attach.
        engine.wizard_enabled = false;
        Ok(engine)
    }

    #[cfg(test)]
    pub(crate) fn test_detach_chart_records(&self) -> DetachedChartTestFixture {
        use tor_simulation::RecordStore;
        let mut game = self.game.clone();
        let mut records = tor_simulation::MemoryRecords::default();
        let detached = tor_simulation::RegionTransition {
            active: BTreeSet::new(),
            loaded: BTreeSet::new(),
        };
        // The paid actor still has its ordinary observing point. Prove its
        // active-region pin before deliberately removing the test clone's
        // observers through the public lifecycle API (not by bypassing pins).
        let before = game.clone();
        assert!(matches!(
            game.apply_region_transition(&detached, &mut records),
            Err(tor_simulation::TransitionError::MustBeActive(_))
        ));
        assert_eq!(game, before);
        assert_eq!(records.ids().count(), 0);
        let paid_history = game
            .chart_history(tor_simulation::ActorId(1))
            .expect("paid fixture chart history")
            .clone();
        let points: Vec<_> = game.reference_points().map(|(id, _)| id).collect();
        assert!(!points.is_empty());
        for point in points {
            assert!(game.remove_reference_point(point));
        }
        // An outstanding controller input boundary is an independent active
        // pin. Leave it through ordinary Wait actions, not private state edits.
        // This fixture's external actors need at most one scheduler cycle.
        let mut waited = BTreeSet::new();
        while !game
            .region_requirements(&detached.active, &detached.loaded)
            .active
            .is_empty()
        {
            let actor = game.next_actor().expect("ready fixture actor");
            assert!(
                waited.insert(actor),
                "fixture input pin failed to discharge"
            );
            game.act(actor, tor_simulation::Action::Wait).unwrap();
        }
        assert_eq!(
            game.chart_history(tor_simulation::ActorId(1)),
            Some(&paid_history),
            "observer removal and ordinary Wait preserve the exact paid history"
        );
        game.apply_region_transition(&detached, &mut records)
            .unwrap();
        let ids: Vec<_> = records.ids().collect();
        let records = ids
            .into_iter()
            .map(|id| (id, records.get(id).unwrap()))
            .collect();
        DetachedChartTestFixture {
            game,
            records,
            package: self.archive.scenario.package.as_ref().unwrap().clone(),
        }
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
    /// Trusted read-only wizard query. Sessions additionally authorize the
    /// requesting account before target lookup and publish only private replies.
    pub fn inspect_creature(&self, actor: ActorId) -> Result<CreatureInspectionView, Failure> {
        self.inspect_creature_at(&self.presentation_boundary(), actor, 0)
    }
    pub fn inspect_creature_at(
        &self,
        boundary: &PresentationBoundary,
        actor: ActorId,
        display_generation: u64,
    ) -> Result<CreatureInspectionView, Failure> {
        self.require_wizard_authority()?;
        let unavailable = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Creature inspection is unavailable",
            )
        };
        let game = self.presentation_game(boundary)?;
        let report = crate::creature_view::inspect(game, SimActor(actor.0), display_generation)
            .ok_or_else(unavailable)?;
        report.validate().map_err(|_| {
            Failure::new(
                ErrorCode::InvalidArchive,
                "Creature inspection is inconsistent",
            )
        })?;
        Ok(report)
    }

    /// Trusted runtime tooling. Session queries separately check the recipient,
    /// observer context and revision; this neither journals nor advances time.
    pub fn configure_combat_diagnostics(
        &mut self,
        enabled: bool,
    ) -> Result<CombatDiagnosticsView, Failure> {
        self.require_wizard_authority()?;
        self.combat_diagnostics_enabled = enabled;
        self.game.set_combat_diagnostics(enabled);
        self.inspect_combat_diagnostics(None)
    }

    /// A private numerical page; authorization precedes page availability checks.
    pub fn inspect_combat_diagnostics(
        &self,
        through: Option<u64>,
    ) -> Result<CombatDiagnosticsView, Failure> {
        self.require_wizard_authority()?;
        let report =
            crate::creature_view::combat_diagnostics(&self.game, through).map_err(|error| {
                use crate::creature_view::DiagnosticProjectionError as E;
                match error {
                    E::InvalidPage => Failure::new(
                        ErrorCode::InvalidAction,
                        "Combat diagnostic page is unavailable",
                    ),
                    E::Inconsistent => Failure::new(
                        ErrorCode::InvalidArchive,
                        "Combat diagnostics are inconsistent",
                    ),
                }
            })?;
        report.validate().map_err(|_| {
            Failure::new(
                ErrorCode::InvalidArchive,
                "Combat diagnostics are inconsistent",
            )
        })?;
        Ok(report)
    }

    fn require_wizard_authority(&self) -> Result<(), Failure> {
        if self.wizard_enabled {
            Ok(())
        } else {
            Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard authority is required",
            ))
        }
    }

    pub fn wizard_enabled(&self) -> bool {
        self.wizard_enabled
    }
    /// Actors currently loaded in the authoritative game.
    pub fn actors(&self) -> Vec<ActorId> {
        self.game
            .loaded_actor_ids()
            .map(|id| ActorId(id.0))
            .collect()
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

    /// Private backend freshness: branch plus append-only journal ordinal.
    /// Never use this value as a public presentation generation.
    pub fn causal_boundary(&self, actor: ActorId) -> Result<u64, Failure> {
        if self.game.known_actor_region(SimActor(actor.0)).is_none() {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Actor is unavailable",
            ));
        }
        u64::try_from(self.archive.records.len()).map_err(|_| invalid_archive())
    }
    pub fn presentation_boundary(&self) -> PresentationBoundary {
        PresentationBoundary {
            boundary: self.boundaries.back().expect("committed boundary").clone(),
            branch: self.branch().clone(),
            owner: self.archive.view_salt.clone(),
            wizard_game: self.archive.wizard_game,
        }
    }
    pub(crate) fn presentation_game<'a>(
        &self,
        boundary: &'a PresentationBoundary,
    ) -> Result<&'a Game, Failure> {
        if boundary.owner != self.archive.view_salt {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Foreign presentation boundary",
            ));
        }
        Ok(&boundary.boundary.game)
    }
    /// Private causal event identity, never serialized into presentation tokens.
    pub(crate) fn event_history_boundary_at(
        &self,
        boundary: &PresentationBoundary,
    ) -> Result<tor_protocol::EventProvenance, Failure> {
        let (batch, through) = self.presentation_game(boundary)?.event_history_boundary();
        Ok(tor_protocol::EventProvenance { batch, through })
    }
    /// Internal semantic read; generation zero is not issued to a client.
    pub fn observation(&self, actor: ActorId) -> Result<Observation, Failure> {
        self.observation_at(&self.presentation_boundary(), actor, 0)
    }
    pub fn observation_at(
        &self,
        boundary: &PresentationBoundary,
        actor: ActorId,
        display_generation: u64,
    ) -> Result<Observation, Failure> {
        let game = self.presentation_game(boundary)?;
        let view = if self
            .boundaries
            .back()
            .is_some_and(|current| Arc::ptr_eq(current, &boundary.boundary))
            && boundary.branch == *self.branch()
        {
            self.revision_view(actor)?
        } else {
            Arc::new(revision_view(game, actor)?)
        };
        self.adapt_observation_at(boundary, actor, display_generation, &view)
    }

    fn adapt_observation_at(
        &self,
        boundary: &PresentationBoundary,
        actor: ActorId,
        display_generation: u64,
        view: &CachedObservation,
    ) -> Result<Observation, Failure> {
        let game = self.presentation_game(boundary)?;
        let _span = tor_simulation::diagnostics::spans::span("protocol_picture_adaptation");
        #[cfg(test)]
        FULL_PICTURE_ADAPTATIONS.with(|count| count.set(count.get() + 1));
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
            &view.scene,
            &self.archive.view_salt,
            view.ready,
            &terrain,
            &self.target_scope(actor),
        );
        observation.charts = view.charts.summaries(
            &self.archive.view_salt,
            actor,
            boundary.branch(),
            display_generation,
        );
        observation.places = game
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
    #[cfg(test)]
    pub(crate) fn picture_adaptation_count() -> usize {
        FULL_PICTURE_ADAPTATIONS.with(|count| count.get())
    }

    pub(crate) fn semantic_observation(
        &self,
        actor: ActorId,
    ) -> Result<SemanticObservation, Failure> {
        let boundary = self.presentation_boundary();
        let view = self.revision_view(actor)?;
        let facts = adapt::outcome_observation(
            &view.observation,
            &view.scene,
            &self.archive.view_salt,
            view.ready,
            &self.target_scope(actor),
        );
        Ok(SemanticObservation {
            facts,
            actor,
            boundary,
            view,
        })
    }

    /// A sufficient equality proof only. A failed proof never itself publishes
    /// a change: the caller must compare the ordinary canonical pictures.
    pub(crate) fn semantic_picture_unchanged(
        &self,
        before: &SemanticObservation,
        after: &SemanticObservation,
    ) -> Result<bool, Failure> {
        if before.actor != after.actor
            || before.boundary.branch != after.boundary.branch
            || before.boundary.owner != after.boundary.owner
            || before.boundary.wizard_game != after.boundary.wizard_game
        {
            return Ok(false);
        }
        let old = self.presentation_game(&before.boundary)?;
        let new = self.presentation_game(&after.boundary)?;
        Ok(old
            .remembered_places(SimActor(before.actor.0))
            .eq(new.remembered_places(SimActor(after.actor.0)))
            && crate::semantic_visual::same_picture_inputs(
                crate::semantic_visual::PictureInputs {
                    observation: &before.view.observation,
                    scene: &before.view.scene,
                    ready: before.view.ready,
                    charts: &before.view.charts,
                },
                crate::semantic_visual::PictureInputs {
                    observation: &after.view.observation,
                    scene: &after.view.scene,
                    ready: after.view.ready,
                    charts: &after.view.charts,
                },
            ))
    }

    pub(crate) fn semantic_picture(
        &self,
        capture: &SemanticObservation,
    ) -> Result<StateView, Failure> {
        Ok(StateView {
            snapshot: SnapshotId("internal-unissued".into()),
            wizard_game: capture.boundary.wizard_game,
            revision: 0,
            observation: self.adapt_observation_at(
                &capture.boundary,
                capture.actor,
                0,
                &capture.view,
            )?,
        })
    }

    #[cfg(test)]
    pub(crate) fn prepared_detection_charts(
        &self,
        actor: ActorId,
    ) -> Result<Arc<crate::chart_view::PreparedCharts>, Failure> {
        self.prepared_detection_charts_at(&self.presentation_boundary(), actor)
    }
    pub(crate) fn prepared_detection_charts_at(
        &self,
        boundary: &PresentationBoundary,
        actor: ActorId,
    ) -> Result<Arc<crate::chart_view::PreparedCharts>, Failure> {
        Ok(Arc::new(crate::chart_view::PreparedCharts::for_game(
            self.presentation_game(boundary)?,
            actor,
            false,
        )?))
    }
    pub fn explored_map(
        &self,
        actor: ActorId,
        window: tor_protocol::MapWindow,
    ) -> Result<crate::PreparedMapProjection, Failure> {
        self.explored_map_at(&self.presentation_boundary(), actor, window)
    }
    pub fn explored_map_at(
        &self,
        boundary: &PresentationBoundary,
        actor: ActorId,
        window: tor_protocol::MapWindow,
    ) -> Result<crate::PreparedMapProjection, Failure> {
        let game = self.presentation_game(boundary)?;
        let normal = tor_protocol::MapWindow::normal();
        if window.validate().is_err()
            || !normal.contains(window.min)
            || !normal.contains(window.max)
        {
            return Err(Failure::new(
                ErrorCode::InvalidAction,
                "Map window is outside the current viewport",
            ));
        }
        let scene = game
            .capture_observation_scene_shared(SimActor(actor.0))
            .map_err(|_| invalid_archive())?;
        let simulation_window = |window: tor_protocol::MapWindow| tor_simulation::MapWindow {
            min: tor_world::Position {
                x: window.min.x,
                y: window.min.y,
                z: window.min.z,
            },
            max: tor_world::Position {
                x: window.max.x,
                y: window.max.y,
                z: window.max.z,
            },
        };
        let projection = game
            .project_explored_map_region(
                SimActor(actor.0),
                &scene,
                simulation_window(window),
                simulation_window(normal),
            )
            .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Invalid map window"))?;
        if projection.appearance_limited {
            return Err(Failure::new(
                ErrorCode::ResourceLimit,
                "Map appearance exceeds its response budget",
            ));
        }
        Ok(crate::PreparedMapProjection {
            cells: projection
                .cells
                .into_iter()
                .map(|c| crate::map_memory::wire_cell(c, &self.archive.view_salt, actor))
                .collect(),
            complete: projection.complete,
            ambiguous: projection.ambiguous,
            limited: projection.limited,
            work: projection.work,
            root: game.map_knowledge_root(SimActor(actor.0)),
        })
    }
    pub fn map_projection_is_current(
        &self,
        actor: ActorId,
        prepared: &crate::PreparedMapProjection,
    ) -> bool {
        self.game.has_actor(SimActor(actor.0))
            && self
                .game
                .map_knowledge_is_current(SimActor(actor.0), &prepared.root)
    }
    fn revision_view(&self, actor: ActorId) -> Result<Arc<CachedObservation>, Failure> {
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

    /// Internal semantic state only. Service issues its own display generation
    /// and opaque snapshot capability through state_at, never this sentinel.
    pub fn state(&self, actor: ActorId) -> Result<StateView, Failure> {
        self.state_at(&self.presentation_boundary(), actor, 0)
    }
    pub fn state_at(
        &self,
        boundary: &PresentationBoundary,
        actor: ActorId,
        display_generation: u64,
    ) -> Result<StateView, Failure> {
        Ok(StateView {
            snapshot: SnapshotId("internal-unissued".into()),
            wizard_game: boundary.wizard_game,
            revision: display_generation,
            observation: self.observation_at(boundary, actor, display_generation)?,
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
        self.causal_boundary(actor)?;
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

    /// Trusted backend issuance path. Unlike wire requests, this caller owns an
    /// immutable presentation handle and explicitly binds its chosen snapshot ID.
    /// Raw receipt identity remains unchanged; retries precede handle freshness.
    #[allow(clippy::too_many_arguments)]
    pub fn command_at_presentation(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
        boundary: &PresentationBoundary,
    ) -> Result<CommandResult, Failure> {
        let receipt = Receipt {
            user: user.into(),
            frontend: frontend.into(),
            actor,
            request_id: request_id.into(),
            branch: branch.clone(),
            command,
        };
        if let Some(result) = self.resolve_receipt(&receipt)? {
            return Ok(result);
        }
        let game = self.presentation_game(boundary)?;
        if boundary.branch() != branch || game.known_actor_region(SimActor(actor.0)).is_none() {
            return Err(invalid_anchor());
        }
        let anchor = match &receipt.command {
            Command::Annotate {
                anchor: Anchor::State { snapshot },
                ..
            } => crate::journal::RecordedAnchor::State {
                snapshot: snapshot.clone(),
                boundary: boundary.entry().cloned(),
            },
            Command::Annotate {
                anchor: Anchor::Entry { id },
                ..
            } => crate::journal::RecordedAnchor::Entry { id: id.clone() },
            _ => return self.apply_command(&receipt, None),
        };
        self.command_with_annotation(&receipt, anchor)
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

    /// Offline aggregate of loaded finite claims. This is deliberately absent
    /// from client wire views and measured ordinary command paths.
    pub fn profile_finite_claims(
        &self,
    ) -> Result<tor_simulation::diagnostics::FiniteClaimProfile, Failure> {
        self.game
            .profile_finite_claims()
            .map_err(|_| storage_failure())
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
        let records = self
            .regions
            .as_ref()
            .map(|regions| regions.encoded_resident())
            .transpose()?
            .unwrap_or_default();
        let (store, _, _, saved) = crate::storage::Store::open(
            &path,
            || Ok((self.archive.clone(), sources, records)),
            policy,
            lock.clone(),
        )?;
        if let Some(package) = &mut self.archive.scenario.package {
            let isolated = Arc::make_mut(package);
            isolated.sources = isolated.sources.independent();
            isolated.attach_persisted_inputs(store.source_reader(), None);
            if let Some(regions) = &mut self.regions {
                regions.bind_package_inputs(package.clone());
            }
        }
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

    /// Measure the current compressed checkpoint without allocating its encoded
    /// payload or writing a save. Includes capture, deduplication and serialization;
    /// intended for offline diagnostics, outside measured action intervals. The
    /// production 64 MiB writer limit remains enforced independently.
    pub fn profile_checkpoint_encoding(&self) -> Result<(u64, Duration), Failure> {
        self.profile_checkpoint_size(true)
    }

    /// Measure logical JSON size before compression, without allocating a payload.
    /// Use this for state-size comparisons and budgets independent of compression.
    pub fn profile_checkpoint_json_encoding(&self) -> Result<(u64, Duration), Failure> {
        self.profile_checkpoint_size(false)
    }

    /// Count one pending frozen Plan's logical JSON bytes without allocating an
    /// encoded payload or recapturing operands. Offline diagnostics only: this
    /// exposes no plan fields and belongs outside measured command intervals.
    pub fn profile_frozen_plan_json_encoding(
        &self,
        actor: ActorId,
    ) -> Result<Option<(u64, Duration)>, Failure> {
        let Some(preparation) = self.game.preparation(SimActor(actor.0)) else {
            return Ok(None);
        };
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
        let started = Instant::now();
        let mut counter = Counter(0);
        serde_json::to_writer(&mut counter, &preparation.plan).map_err(|_| storage_failure())?;
        Ok(Some((counter.0, started.elapsed())))
    }

    fn profile_checkpoint_size(&self, compressed: bool) -> Result<(u64, Duration), Failure> {
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
            self.store
                .as_ref()
                .map(|store| store.save_id())
                .unwrap_or("00000000-0000-0000-0000-000000000000"),
            self.archive.records.len() as u64,
        );
        let mut counter = Counter(0);
        if compressed {
            crate::storage::checkpoint_codec::write_json(&encoded, &mut counter)
                .map_err(|_| storage_failure())?;
        } else {
            serde_json::to_writer(&mut counter, &encoded).map_err(|_| storage_failure())?;
        }
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
        if !self.input_generations.is_empty() {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Diagnostic seeding cannot run with attached input observers",
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
            let expected_boundary = self.causal_boundary(actor)?;
            let tick = self.game.tick();
            let activation_count = self.game.activation_count();
            let retrieval_before = RetrievalBoundaryState::capture(&self.game);
            self.game.begin_retrieval_transaction();
            let outcome = self
                .game
                .act(SimActor(actor.0), tor_simulation::Action::Wait)
                .map_err(|_| invalid_archive())?;
            // Wait changes only time/readiness. Geometry and navigation are
            // identical; equivalence tests compare the complete Game and every
            // retained boundary against ordinary commands. Never use this
            // shortcut for measured actions or an engine attached to a save.
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
                    expected_boundary,
                    action: Action::Wait,
                },
            };
            self.append_record(Record {
                retrieval_before,
                retrieval_after: RetrievalBoundaryState::capture(&self.game),
                retrievals: self.game.retrieval_entries().to_vec(),
                before_tick: tick,
                after_tick: self.game.tick(),
                entry: entry.clone(),
                receipt: Some(receipt),
                action_resolution: ActionResolution::Unchanged,
                activations: self
                    .game
                    .activations_since(activation_count)
                    .map_err(|_| invalid_archive())?,
            });
            if end - index <= REWIND_BOUNDARIES {
                self.boundaries.push_back(Arc::new(Boundary {
                    id: Some(entry.id),
                    selectable: true,
                    game: self.game.clone(),
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
        self.apply_command_profiled_with_annotation(receipt, recorded_id, profile, None)
    }
    pub(crate) fn command_with_annotation(
        &mut self,
        receipt: &Receipt,
        anchor: crate::journal::RecordedAnchor,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled_with_annotation(receipt, None, None, Some(anchor))
    }
    fn apply_command_with_annotation(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
        annotation: Option<crate::journal::RecordedAnchor>,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled_with_annotation(receipt, recorded_id, None, annotation)
    }
    fn apply_command_profiled_with_annotation(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
        annotation: Option<crate::journal::RecordedAnchor>,
    ) -> Result<CommandResult, Failure> {
        let started = profile.as_ref().map(|_| Instant::now());
        if let Some(result) = self.resolve_receipt(receipt)? {
            if let Some(profile) = profile.as_deref_mut() {
                profile.request_validation += started.expect("profile clock").elapsed();
            }
            return Ok(result);
        }
        let checked = self.check_request(receipt)?;
        let checked = match annotation {
            Some(anchor) => checked.with_annotation_anchor(anchor)?,
            None => checked,
        };
        if let Some(profile) = profile.as_deref_mut() {
            profile.request_validation += started.expect("profile clock").elapsed();
        }
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
            regions.appended(store.take_appended());
        }
        let started = Instant::now();
        let mut candidate = Candidate::capture(self);
        candidate.game.begin_retrieval_transaction();
        if let Some(profile) = profile {
            profile.rollback_capture += started.elapsed();
            profile.candidate_captures += 1;
        }
        candidate
    }

    /// Apply/reconcile one action inside a private candidate. A supplied outcome
    /// comes only from an uninterrupted simulation execution against that candidate.
    fn resolve_action_transition(
        &mut self,
        candidate: &mut Candidate,
        actor: ActorId,
        action: &Action,
        executed: Option<tor_simulation::ActionOutcome>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<tor_simulation::ActionOutcome, Failure> {
        let mut navigation_refreshed = false;
        let geometry = self.game.geometry_snapshot();
        let started = Instant::now();
        let perception_changed = match action {
            Action::Wait => self.game.wait_changes_perception(SimActor(actor.0)),
            Action::Move { .. }
            | Action::Escape { .. }
            | Action::ChooseGrowthSource { .. }
            | Action::ChooseIntellectExpertise { .. }
            | Action::Purchase { .. }
            | Action::Respec { .. }
            | Action::UseAbility { .. }
            | Action::Equip { .. }
            | Action::Unequip { .. }
            | Action::Ignite { .. }
            | Action::Extinguish { .. }
            | Action::Drink { .. }
            | Action::Eat { .. }
            | Action::Read { .. }
            | Action::Zap { .. }
            | Action::Identify { .. }
            | Action::ExamineTrap { .. }
            | Action::SetDoor { .. }
            | Action::Take { .. }
            | Action::TouchApparentObject { .. }
            | Action::Drop { .. } => true,
        };
        // A live whole-scope chart can change outside ordinary sight, even on
        // a same-tick wait or condition expiry. The predicate reads only the
        // active marker; the canonical census runs once in revision_view below.
        let perception_changed = perception_changed
            || self
                .actors()
                .into_iter()
                .any(|observer| self.game.monster_detection_active(SimActor(observer.0)));
        let observers: Vec<_> = self
            .actors()
            .into_iter()
            .filter(|_| perception_changed)
            .collect();
        if let Some(profile) = profile.as_deref_mut() {
            profile.perception += started.elapsed();
        }
        let started = Instant::now();
        let already_executed = executed.is_some();
        let step = match executed {
            Some(outcome) => CandidateStep::Settled(outcome),
            None => candidate
                .game
                .act_for_host(SimActor(actor.0), adapt::action(action))
                .map_err(|error| {
                    Failure::simulation_failure(error).unwrap_or_else(|| {
                        Failure::new(ErrorCode::InvalidAction, "Action is unavailable")
                    })
                })?,
        };
        if let Some(profile) = profile.as_deref_mut().filter(|_| !already_executed) {
            profile.simulation_transition += started.elapsed();
            profile.simulation_transitions += 1;
        }
        let mut outcome = match step {
            CandidateStep::Settled(outcome) => outcome,
            CandidateStep::AwaitingRegions(outcome) => {
                candidate.settle_action_entry(self.regions.as_mut(), profile.as_deref_mut())?;
                outcome
            }
        };
        outcome.next_tick = candidate.game.tick();
        outcome.next_actor = candidate.game.next_actor();
        let event_witness = candidate.game.event_observation_witness();
        // Stationary waits usually change only tick/readiness. Capability
        // expiry can change disclosed traversal without any geometry change.
        if !perception_changed {
            let started = Instant::now();
            let mut capability_changes = Vec::new();
            for observer in candidate.actors() {
                if self.game.actor_locomotion(SimActor(observer.0))
                    != candidate.game.actor_locomotion(SimActor(observer.0))
                {
                    capability_changes.push(observer);
                }
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.revision_detection += started.elapsed();
            }
            for observer in capability_changes {
                if let Some(profile) = profile.as_deref_mut() {
                    profile.action_observer_boundaries += 1;
                }
                let started = Instant::now();
                candidate
                    .game
                    .validate_observer_projection(SimActor(observer.0))
                    .map_err(|_| invalid_archive())?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.action_projection_validated += 1;
                }
                let facts = candidate
                    .game
                    .capture_commit_event_facts(SimActor(observer.0))
                    .map_err(|_| invalid_archive())?;
                candidate
                    .game
                    .record_commit_event_facts(&event_witness, &facts)
                    .map_err(|_| invalid_archive())?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.action_event_receipts_recorded += 1;
                    profile.perception += started.elapsed();
                }
                let started = Instant::now();
                candidate
                    .game
                    .refresh_navigation_scene(SimActor(observer.0), facts.scene());
                navigation_refreshed = true;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.navigation_refresh += started.elapsed();
                    profile.action_navigation_reviewed += 1;
                }
            }
        }
        // Navigation depends on geometry and each observer's scene, not the
        // action kind or changes to entities/readiness. An unsettled same-tick
        // physics handoff can change disclosure without changing known links.
        let navigation_changed = candidate.game.geometry_snapshot() != geometry;
        // Capture event-time authority synchronously, without making presentation
        // DTOs. All receipts bind before the immutable dependency comparison.
        let mut scenes = Vec::with_capacity(observers.len());
        let perception_started = Instant::now();
        for observer in observers {
            if !candidate.game.has_actor(SimActor(observer.0)) {
                continue;
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.action_observer_boundaries += 1;
            }
            let old_scene = self
                .game
                .capture_observation_scene_shared(SimActor(observer.0))
                .map_err(|_| invalid_archive())?;
            if let Some(profile) = profile.as_deref_mut() {
                profile.previous_observer_scene_reads += 1;
            }
            let current = candidate
                .game
                .capture_commit_event_facts(SimActor(observer.0))
                .map_err(|_| invalid_archive())?;
            candidate
                .game
                .record_commit_event_facts(&event_witness, &current)
                .map_err(|_| invalid_archive())?;
            if let Some(profile) = profile.as_deref_mut() {
                profile.action_event_receipts_recorded += 1;
            }
            scenes.push((observer, old_scene, current.scene().clone()));
        }
        if let Some(profile) = profile.as_deref_mut() {
            profile.perception += perception_started.elapsed();
        }
        for (observer, old_scene, current_scene) in scenes {
            self.game
                .validate_observer_projection(SimActor(observer.0))
                .map_err(|_| invalid_archive())?;
            candidate
                .game
                .validate_observer_projection(SimActor(observer.0))
                .map_err(|_| invalid_archive())?;
            if let Some(profile) = profile.as_deref_mut() {
                profile.action_projection_validated += 1;
            }
            if navigation_changed
                || self.game.actor_locomotion(SimActor(observer.0))
                    != candidate.game.actor_locomotion(SimActor(observer.0))
                || candidate
                    .game
                    .navigation_scene_changed(&old_scene, &current_scene)
            {
                navigation_refreshed = true;
                let started = Instant::now();
                candidate
                    .game
                    .refresh_navigation_scene(SimActor(observer.0), &current_scene);
                if let Some(profile) = profile.as_deref_mut() {
                    profile.navigation_refresh += started.elapsed();
                }
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.action_navigation_reviewed += 1;
            }
        }
        if navigation_refreshed {
            if let Some(profile) = profile.as_deref_mut() {
                profile.navigation_refreshes += 1;
            }
        }
        candidate.reconcile_actors(&self.game, profile)?;
        Ok(outcome)
    }

    fn finish_checked_command(
        &mut self,
        checked: command_request::CheckedRequest<'_>,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
        mut candidate: Candidate,
    ) -> Result<CommandResult, Failure> {
        let (receipt, _boundary, annotation) = checked.into_parts();
        let mut action_resolution = ActionResolution::Unchanged;
        let mut tick = self.game.tick();
        let entry_id = recorded_id.unwrap_or_else(new_id);
        let (author, audience, content) = match &receipt.command {
            Command::ResumeIntention { admission, .. }
            | Command::CancelIntention { admission, .. } => {
                let change = match &receipt.command {
                    Command::ResumeIntention { .. } => crate::journal::IntentionChange::Resumed,
                    Command::CancelIntention {
                        step: Some(direction),
                        ..
                    } => crate::journal::IntentionChange::Escaped {
                        direction: *direction,
                    },
                    Command::CancelIntention { step: None, .. } => {
                        crate::journal::IntentionChange::Cancelled
                    }
                    _ => unreachable!("matched intention control"),
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
                let (action, resolution) =
                    ActionResolution::resolve(&candidate.game, receipt.actor, action);
                action_resolution = resolution;
                let intention = candidate
                    .game
                    .admit_intention(
                        SimActor(receipt.actor.0),
                        adapt::action(&action),
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
                        human_operands: candidate
                            .game
                            .pending_intention(SimActor(receipt.actor.0))
                            .and_then(|queued| queued.human_operands.clone())
                            .ok_or_else(invalid_archive)?,
                        action: action.into_owned(),
                    },
                )
            }
            Command::PausePreparation => {
                let work = candidate
                    .game
                    .pause_preparation(SimActor(receipt.actor.0))
                    .ok_or_else(|| {
                        Failure::new(ErrorCode::InvalidAction, "No active preparation")
                    })?;
                (
                    Author::Backend {
                        component: "scheduler".into(),
                    },
                    Audience::Actor,
                    JournalContent::Action {
                        action: adapt::recorded_action(work.action()).unwrap(),
                        event: crate::journal::Event::PreparationPaused,
                    },
                )
            }
            Command::RenamePlace {
                display_basis: _,
                expected_boundary: _,
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
                display_basis: _,
                expected_boundary: _,
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
                display_basis: _,
                expected_boundary: _,
                operation,
            } => {
                candidate.load_for_wizard(self.regions.as_mut(), operation)?;
                // The preload transition may retain views, but the wizard
                // operation now mutates that boundary (or restores a rewind).
                candidate.observations.clear();
                let result = candidate.apply_wizard(
                    receipt,
                    operation,
                    &entry_id,
                    &self.archive.records,
                    self.regions.as_mut(),
                )?;
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
                expected_boundary: _,
                action,
            } => {
                let (action, resolution) =
                    ActionResolution::resolve(&candidate.game, receipt.actor, action);
                action_resolution = resolution;
                let outcome = self.resolve_action_transition(
                    &mut candidate,
                    receipt.actor,
                    &action,
                    None,
                    profile.as_deref_mut(),
                )?;
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    JournalContent::Action {
                        action: action.into_owned(),
                        event: adapt::event(outcome.kind),
                    },
                )
            }
            Command::Annotate {
                anchor: _,
                text,
                source,
                audience,
                category,
            } => {
                let anchor = annotation.as_ref().ok_or_else(invalid_anchor)?;
                self.validate_note(receipt.actor, &receipt.user, *audience, anchor, text)?;
                // An annotation owns no gameplay boundary. Keep the preceding
                // entry evidence with the unchanged authoritative snapshot.
                candidate.game = self.game.clone();
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
                | Command::CancelIntention { step: None, .. }
        ) {
            candidate.transition(self.regions.as_mut(), profile.as_deref_mut())?;
        }
        let started = Instant::now();
        if matches!(
            receipt.command,
            Command::Wizard { .. } | Command::CancelIntention { step: Some(_), .. }
        ) {
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
        self.commit_candidate_with_resolution(
            candidate,
            entry,
            Some(receipt.clone()),
            action_resolution,
            profile,
        )
    }

    fn commit_candidate(
        &mut self,
        candidate: Candidate,
        entry: JournalEntry,
        receipt: Option<Receipt>,
        profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        self.commit_candidate_with_resolution(
            candidate,
            entry,
            receipt,
            ActionResolution::Unchanged,
            profile,
        )
    }

    fn commit_candidate_with_resolution(
        &mut self,
        mut candidate: Candidate,
        mut entry: JournalEntry,
        receipt: Option<Receipt>,
        action_resolution: ActionResolution,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        if candidate.game.region_transition_pending() {
            return Err(invalid_archive());
        }
        candidate
            .game
            .set_combat_diagnostics(self.combat_diagnostics_enabled);
        let started = profile.as_ref().map(|_| Instant::now());
        let changes = intention::derive_intention_changes(
            &self.game,
            &candidate.game,
            &entry,
            candidate.branch() == self.branch(),
        );
        if let Some(profile) = profile.as_deref_mut() {
            profile.intention_derivation += started.expect("profile clock").elapsed();
            profile.intention_actor_visits += changes.actor_visits;
            profile.intention_candidate_visits += changes.candidate_visits;
            profile.intention_event_visits += changes.event_visits;
        }
        entry.intention_suspensions = changes.suspensions;
        entry.intention_ends = changes.ends;
        // A no-residency-change entry can still create actors. Ordinary actions
        // with no entry keep their existing cost rather than rescan membership.
        if candidate.game.activation_count() != self.game.activation_count() {
            candidate.reconcile_actors(&self.game, profile.as_deref_mut())?;
        }
        let metadata_started = profile.as_ref().map(|_| Instant::now());
        // Rewind restores a previous ledger. That restoration is not an entry
        // and must never look like a fresh activation in the new branch.
        let activations = if candidate.branch() == self.branch() {
            candidate
                .game
                .activations_since(self.game.activation_count())
                .map_err(|_| invalid_archive())?
        } else {
            Vec::new()
        };
        if candidate.branch() != self.branch() {
            candidate.game.begin_retrieval_transaction();
        }
        let record = Record {
            retrieval_before: RetrievalBoundaryState::capture(&self.game),
            retrieval_after: RetrievalBoundaryState::capture(&candidate.game),
            retrievals: if matches!(entry.content, JournalContent::Annotation { .. }) {
                Vec::new()
            } else {
                candidate.game.retrieval_entries().to_vec()
            },
            before_tick: self.game.tick(),
            after_tick: candidate.game.tick(),
            entry: entry.clone(),
            receipt,
            action_resolution,
            activations,
        };
        let survival = candidate.game.take_survival_diagnostics();
        let start_tick = self.game.tick();
        let branch_changed = candidate.branch() != self.branch();
        // Admission changes queue identity only. Already-built committed views
        // remain valid across this boundary and knowledge can reuse them.
        if entry.content.admission().is_some() {
            candidate.observations = self.observations.snapshot();
        }
        if let Some(profile) = profile.as_deref_mut() {
            profile.commit_metadata += metadata_started.expect("profile clock").elapsed();
        }
        if !matches!(entry.content, JournalContent::Annotation { .. }) {
            let terrain = |region: tor_world::RegionId| {
                self.archive
                    .scenario
                    .package
                    .as_deref()
                    .and_then(|p| p.region_terrain(region.0))
                    .map_or([None, None, None], |t| {
                        [t.floor.clone(), t.wall.clone(), t.door.clone()]
                    })
            };
            candidate.record_explored_knowledge(&self.game, &terrain, profile.as_deref_mut())?;
            candidate.record_observation_knowledge(profile.as_deref_mut())?;
            let started = Instant::now();
            candidate.boundaries.push_back(Arc::new(Boundary {
                id: Some(entry.id.clone()),
                selectable: entry.content.rewindable(),
                game: candidate.game.clone(),
            }));
            retain_boundaries(&mut candidate.boundaries, |b| b.selectable);
            if let Some(profile) = profile.as_deref_mut() {
                profile.rollback_snapshot += started.elapsed();
                profile.rollback_snapshots += 1;
            }
        }
        let started = profile.as_ref().map(|_| Instant::now());
        let input_generations = self.prepare_input_generations(&candidate, &entry);
        if let Some(profile) = profile.as_deref_mut() {
            profile.input_generation += started.expect("profile clock").elapsed();
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
        self.publish_input_generations(input_generations);
        if self.survival_log.is_some() {
            self.emit_survival(serde_json::json!({
                "event":"survival.commit", "start_tick":start_tick, "end_tick":self.game.tick(),
                "actor":entry.actor.0, "branch_changed":branch_changed,
                "flows":survival.unwrap_or_default(),
            }));
        }
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
        let anchor = match anchor {
            Anchor::Entry { id } => crate::journal::RecordedAnchor::Entry { id },
            Anchor::State { snapshot } => crate::journal::RecordedAnchor::State {
                snapshot,
                boundary: self.presentation_boundary().entry().cloned(),
            },
        };
        self.backend_note(actor, component, anchor, category, text, None)
    }

    fn backend_note(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: crate::journal::RecordedAnchor,
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
            retrieval_before: RetrievalBoundaryState::capture(&self.game),
            retrieval_after: RetrievalBoundaryState::capture(&self.game),
            before_tick: self.game.tick(),
            after_tick: self.game.tick(),
            entry: entry.clone(),
            receipt: None,
            action_resolution: ActionResolution::Unchanged,
            activations: Vec::new(),
            retrievals: Vec::new(),
        };
        let copied = self.admit(&record, &Candidate::capture(self), None)?;
        self.mark_saved(copied);
        self.append_record(record);
        if self.survival_log.is_some() {
            self.emit_survival(serde_json::json!({
                "event":"survival.commit", "start_tick":self.game.tick(), "end_tick":self.game.tick(),
                "actor":entry.actor.0, "branch_changed":false,
                "flows":tor_simulation::survival_diagnostics::SurvivalBatch::default(),
            }));
        }
        Ok(entry)
    }

    fn validate_note(
        &self,
        actor: ActorId,
        user: &str,
        audience: Audience,
        anchor: &crate::journal::RecordedAnchor,
        text: &str,
    ) -> Result<(), Failure> {
        self.causal_boundary(actor)?;
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
            crate::journal::RecordedAnchor::State { snapshot, boundary } => {
                if !snapshot.is_valid() {
                    return Err(invalid_anchor());
                }
                match boundary {
                    Some(id) => {
                        let index = self.history_index.find(id).ok_or_else(invalid_anchor)?;
                        if self.archive.records[index].entry.branch != *self.branch() {
                            return Err(invalid_anchor());
                        }
                    }
                    None if self.branch() != &self.archive.branch => return Err(invalid_anchor()),
                    None => {}
                }
                Ok(())
            }
            crate::journal::RecordedAnchor::Entry { id } => {
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
            combat_diagnostics_enabled: self.combat_diagnostics_enabled,
            survival_log: None,
            boundaries: self.boundaries.clone(),
            game: self.game.clone(),
            archive: self.archive.clone(),

            receipts: self.receipts.clone(),
            observations: ObservationCache::default(),
            input_generations: InputGenerations::default(),
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
        u64::try_from(self.archive.records.len())
            .ok()
            .and_then(|ordinal| ordinal.checked_add(1))
            .ok_or_else(invalid_archive)?;
        // This bounded invariant runs before either persistence enqueue or
        // candidate publication. Rewind's target was validated by apply_wizard;
        // restoring it is not an entry and cannot carry a new receipt batch.
        if !record.valid_action_resolution_at(&self.game)
            || record.retrieval_before != RetrievalBoundaryState::capture(&self.game)
            || record.retrieval_after != RetrievalBoundaryState::capture(&candidate.game)
        {
            return Err(invalid_archive());
        }
        if matches!(
            record.entry.content,
            JournalContent::Wizard {
                operation: WizardOperation::Rewind { .. },
                ..
            }
        ) {
            if !record.retrievals.is_empty() {
                return Err(invalid_archive());
            }
        } else if fold_retrieval_entries(record.retrieval_before.clone(), &record.retrievals)?
            != record.retrieval_after
        {
            return Err(invalid_archive());
        }
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
            let history_admission = candidate.game.chart_history_changed(&self.game);
            store.enqueue_regions(record, sources, &candidate.made, history_admission, || {
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
    if let Some(arena) = &package.manifest.arena {
        for actor in &arena.participants {
            if *actor != package.selected {
                game.add_reference_point(tor_simulation::ReferencePoint {
                    target: tor_simulation::ReferenceTarget::Actor(SimActor(*actor)),
                    active_radius: Some(0),
                    load_radius: Some(0),
                    observes: false,
                })
                .map_err(|_| invalid_archive())?;
            }
        }
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
    regions.transition_initial(&mut game, &mut made)?;
    package.initialize_selected_character(&mut game)?;
    regions.transition(&mut game, &mut made)?;
    regions.publish(made);
    package.configure_loaded_arena(&mut game)?;
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
    fn rebuild_wizard_creature(
        &mut self,
        actor: SimActor,
        build: tor_simulation::creatures::CreatureBuild,
    ) -> Result<(), Failure> {
        let invalid = |_| {
            Failure::new(
                ErrorCode::InvalidAction,
                "Wizard target or settings are unavailable",
            )
        };
        // Retained terminal evidence is immutable. Active arena edits freeze
        // execution without advancing clocks or refunding paid preparation.
        if self.game.arena_run().is_some() {
            self.game.control_arena(true, 0).map_err(invalid)?;
        }
        self.game.rebuild_creature(actor, build).map_err(invalid)?;
        Ok(())
    }

    fn apply_wizard(
        &mut self,
        receipt: &Receipt,
        operation: &WizardOperation,
        entry_id: &EntryId,
        history: &[Record],
        regions: Option<&mut crate::regions::Regions>,
    ) -> Result<WizardResult, Failure> {
        let invalid = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Wizard target or settings are unavailable",
            )
        };
        let before = self.actors();
        for actor in &before {
            self.game
                .validate_observer_projection(SimActor(actor.0))
                .map_err(|_| invalid_archive())?;
        }
        let result = match operation {
            WizardOperation::AdvanceCreature { actor, advancement } => {
                use crate::journal::CreatureAdvancement;
                let id = SimActor(actor.0);
                let mut build = self.game.creature(id).ok_or_else(invalid)?.build().clone();
                let owner_index = |owner: u64| -> Result<tor_simulation::HdIndex, Failure> {
                    owner
                        .checked_sub(1)
                        .map(tor_simulation::HdIndex)
                        .ok_or_else(invalid)
                };
                let purchase_index = |purchase: u16| -> Result<usize, Failure> {
                    purchase.checked_sub(1).map(usize::from).ok_or_else(invalid)
                };
                match advancement {
                    CreatureAdvancement::AddHitDie { source } => {
                        let seed =
                            crate::creature_authoring::actor_health_seed(self.game.seed(), actor.0);
                        build.add_hit_die(
                            crate::creature_authoring::hit_die_source(
                                source,
                                build.content().rules(),
                            )?,
                            seed,
                        )
                    }
                    CreatureAdvancement::Train { owner, skill } => build.train(
                        owner_index(*owner)?,
                        crate::creature_authoring::skill(skill)?,
                    ),
                    CreatureAdvancement::IncreaseAttribute { owner, attribute } => build
                        .increase_attribute(
                            owner_index(*owner)?,
                            crate::creature_authoring::attribute(attribute)?,
                        ),
                    CreatureAdvancement::SelectExpertise { owner, expertise } => build
                        .select_expertise(
                            owner_index(*owner)?,
                            crate::creature_authoring::expertise(
                                expertise,
                                build.content().rules(),
                            )?,
                        ),
                    CreatureAdvancement::SelectTalent { owner, talent } => build.select_talent(
                        owner_index(*owner)?,
                        crate::creature_authoring::talent(talent, build.content().rules())?,
                    ),
                    CreatureAdvancement::ReplacePurchase {
                        owner,
                        purchase,
                        replacement,
                    } => {
                        use crate::journal::PurchaseReplacement as R;
                        use tor_simulation::creatures::{
                            AdvancementPurchase, PurchaseLocation, PurchaseSelection,
                        };
                        let point_pool = |value: &str| {
                            tor_simulation::content_ids::PointPoolId::new(value)
                                .map_err(|_| invalid())
                        };
                        let replacement = match replacement {
                            R::Refund {} => None,
                            R::SkillRank { pool, skill } => Some(AdvancementPurchase {
                                id: None,
                                pool: point_pool(pool)?,
                                selection: PurchaseSelection::SkillRank(
                                    crate::creature_authoring::skill(skill)?,
                                ),
                            }),
                            R::Expertise { pool, expertise } => Some(AdvancementPurchase {
                                id: None,
                                pool: point_pool(pool)?,
                                selection: PurchaseSelection::Expertise(
                                    crate::creature_authoring::expertise(
                                        expertise,
                                        build.content().rules(),
                                    )?,
                                ),
                            }),
                            R::Talent { pool, talent } => Some(AdvancementPurchase {
                                id: None,
                                pool: point_pool(pool)?,
                                selection: PurchaseSelection::Talent(
                                    crate::creature_authoring::talent(
                                        talent,
                                        build.content().rules(),
                                    )?,
                                ),
                            }),
                        };
                        build
                            .preview_replacement(
                                PurchaseLocation {
                                    owner: owner_index(*owner)?,
                                    index: purchase_index(*purchase)?,
                                },
                                replacement,
                            )
                            .and_then(|preview| build.commit_replacement(preview))
                    }
                }
                .map_err(|_| invalid())?;
                self.rebuild_wizard_creature(id, build)?;
                WizardResult::CreatureAdvanced {
                    actor: *actor,
                    advancement: advancement.clone(),
                }
            }
            WizardOperation::SetCreatureTemplate {
                actor,
                template,
                enabled,
            } => {
                let definition = regions
                    .as_deref()
                    .and_then(|regions| regions.creature_catalog().template(template))
                    .ok_or_else(invalid)?;
                let id = SimActor(actor.0);
                let mut build = self.game.creature(id).ok_or_else(invalid)?.build().clone();
                let mut templates = build.template_ids().to_vec();
                let existing = templates
                    .iter()
                    .position(|value| value.as_str() == template);
                if *enabled {
                    if existing.is_some() {
                        return Err(invalid());
                    }
                    templates.push(definition.id.clone());
                } else {
                    templates.remove(existing.ok_or_else(invalid)?);
                }
                build.set_templates(templates).map_err(|_| invalid())?;
                self.rebuild_wizard_creature(id, build)?;
                WizardResult::CreatureTemplateSet {
                    actor: *actor,
                    template: template.clone(),
                    enabled: *enabled,
                }
            }
            WizardOperation::RemoveCreatureHitDie { actor } => {
                let actor = SimActor(actor.0);
                let mut build = self
                    .game
                    .creature(actor)
                    .ok_or_else(invalid)?
                    .build()
                    .clone();
                build.remove_latest().ok_or_else(invalid)?;
                self.rebuild_wizard_creature(actor, build)?;
                WizardResult::CreatureHitDieRemoved {
                    actor: ActorId(actor.0),
                }
            }
            WizardOperation::ArenaControl { paused, advance } => {
                self.game
                    .control_arena(*paused, *advance)
                    .map_err(|_| invalid())?;
                WizardResult::ArenaControlled {
                    paused: *paused,
                    advance: *advance,
                }
            }
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
                // Wizard stock is ordinary, but a source may reserve one of
                // these identities. A display-name match alone is harmless.
                if self.game.unique_retrieval_policy().is_some_and(|policy| {
                    policy.item.archetype == name || policy.item.identity == name
                }) {
                    return Err(invalid());
                }
                let item = self
                    .game
                    .place_item(adapt::location(*position), name.into())
                    .map_err(|_| invalid())?;
                WizardResult::ItemPlaced { item: item.0 }
            }
            WizardOperation::SpawnCreature {
                selector,
                position,
                depth,
            } => {
                let regions = regions.as_deref().ok_or_else(invalid)?;
                let depth = depth.or_else(|| regions.authored_depth(position.region));
                let recipe =
                    crate::wizard_creatures::resolve(&self.game, regions, selector, depth)?;
                let capacity = |error| {
                    if Failure::simulation_failure(error).is_some() {
                        Failure::new(
                            ErrorCode::ResourceLimit,
                            "Creature spawn exceeds current simulation capacity; no creature or stock was created",
                        )
                    } else {
                        invalid()
                    }
                };
                let (actor, selector, depth, carried_item) = match recipe {
                    crate::wizard_creatures::ResolvedCreature::Population {
                        profile,
                        stage,
                        archetype,
                        selector,
                    } => {
                        let (actor, stock) = self
                            .game
                            .spawn_named_population_actor(
                                &profile,
                                &stage,
                                &archetype,
                                depth,
                                adapt::location(*position),
                            )
                            .map_err(capacity)?;
                        let item = match stock {
                            Some(
                                tor_simulation::floor_activation::CarriedLootDecision::Carried {
                                    item,
                                    ..
                                },
                            ) => Some(item.0),
                            _ => None,
                        };
                        (actor, selector, depth, item)
                    }
                    crate::wizard_creatures::ResolvedCreature::Standalone {
                        selector,
                        definition,
                    } => {
                        let actor = self
                            .game
                            .spawn_standalone_creature(
                                &definition.actor,
                                adapt::location(*position),
                                NonZeroU64::new(definition.turn_ticks).ok_or_else(invalid)?,
                                crate::creature_authoring::actor_health_seed,
                            )
                            .map_err(capacity)?;
                        (actor, selector, None, None)
                    }
                };

                WizardResult::CreatureSpawned {
                    actor: ActorId(actor.0),
                    selector,
                    depth,
                    carried_item,
                }
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

                WizardResult::ActorSpawned {
                    actor: ActorId(actor.0),
                }
            }
            WizardOperation::Teleport { actor, position } => {
                match self
                    .game
                    .teleport_for_host(SimActor(actor.0), adapt::location(*position))
                    .map_err(|_| invalid())?
                {
                    CandidateStep::Settled(()) => {}
                    CandidateStep::AwaitingRegions(()) => {
                        self.settle_action_entry(regions, None)?;
                    }
                }
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
                    .game
                    .known_actor_region(SimActor(receipt.actor.0))
                    .is_none()
                {
                    return Err(invalid());
                }
                let later = std::mem::replace(&mut self.game, boundary.game.clone());
                // Retained futures can still refer to their entities and work;
                // new entities and work must never reuse those identities.
                self.game.continue_identities(&later);
                let from_branch = self.current_branch.clone();
                self.current_branch = BranchId(entry_id.0.clone());
                return Ok(WizardResult::Rewound {
                    from_branch,
                    branch: self.current_branch.clone(),
                    tick: self.game.tick(),
                    next_actor: ActorId(self.game.next_scheduled_actor().ok_or_else(invalid)?.0),
                });
            }
        };
        for actor in before {
            if self.game.has_actor(SimActor(actor.0)) {
                self.game
                    .validate_observer_projection(SimActor(actor.0))
                    .map_err(|_| invalid_archive())?;
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
                let snapshot = SnapshotId("history-fixture-genesis".into());
                let receipt = Receipt {
                    user: user.into(),
                    frontend: "history-index".into(),
                    actor: ActorId(1),
                    request_id: request.into(),
                    branch: engine.branch().clone(),
                    command: Command::Annotate {
                        anchor: Anchor::State {
                            snapshot: snapshot.clone(),
                        },
                        category: AnnotationCategory::Note,
                        source: ClientSource::User,
                        audience,
                        text: request.into(),
                    },
                };
                engine
                    .command_with_annotation(
                        &receipt,
                        crate::journal::RecordedAnchor::State {
                            snapshot,
                            boundary: None,
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
                    engine.causal_boundary(id).unwrap(),
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
                expected_boundary: engine.causal_boundary(actor).unwrap(),
                action: action.clone(),
            },
        );
        let expected = reference.act(SimActor(actor.0), adapt::action(&action));
        assert_eq!(result.is_ok(), expected.is_ok());
        if expected.is_ok() {
            reference.refresh_navigation();
            // The full-view oracle must also commit the appearances disclosed
            // by those views, as the ordinary Engine transaction does.
            let terrain = |region: tor_world::RegionId| {
                engine
                    .archive
                    .scenario
                    .package
                    .as_deref()
                    .and_then(|p| p.region_terrain(region.0))
                    .map_or([None, None, None], |t| {
                        [t.floor.clone(), t.wall.clone(), t.door.clone()]
                    })
            };
            for observer in engine.actors() {
                let view = revision_view(&reference, observer).unwrap();
                reference
                    .record_explored_observation(
                        SimActor(observer.0),
                        &crate::map_memory::inputs(&view.observation, &terrain),
                        &view.scene,
                    )
                    .unwrap();
            }
        }
        assert_eq!(
            engine.game, reference,
            "optimized navigation must equal a full refresh"
        );
        for (id, boundary, _old) in before {
            assert_eq!(
                engine.causal_boundary(id).unwrap(),
                boundary + u64::from(result.is_ok())
            );
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
            expected_boundary: 0,
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
            assert_eq!(engine.causal_boundary(ActorId(1)).unwrap(), 0);
        }
    }
}
#[cfg(test)]
mod frozen_plan_profile_tests {
    use super::*;

    #[test]
    fn ordered_strike_plan_byte_profile_is_exact_and_read_only() {
        use crate::scenario_package::{Manifest, Package, RegionDef};
        let mut manifest: Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        manifest.characters[0].turn_ticks = 150;
        let mut region: RegionDef = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/regions/1.toml"
        ))
        .unwrap();
        region.actors.push(
            serde_json::from_value(serde_json::json!({
                "id":2,"at":[2,1,0],"turn_ticks":150,"controller":"external",
                "creature":manifest.characters[0].creature
            }))
            .unwrap(),
        );
        let package = Package::from_parts(
            manifest,
            vec![region],
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap();
        let scenario = Scenario {
            actors: vec![],
            regions: 1,
            package: Some(Arc::new(package)),
            ..Scenario::two_room(42)
        };
        let mut engine = Engine::memory(scenario).unwrap();
        let actor = ActorId(1);
        assert!(engine
            .profile_frozen_plan_json_encoding(actor)
            .unwrap()
            .is_none());
        engine
            .command(
                "profile",
                "test",
                actor,
                "start",
                &engine.branch().clone(),
                Command::Act {
                    expected_boundary: engine.causal_boundary(actor).unwrap(),
                    action: Action::UseAbility {
                        id: tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap(),
                        invocation: crate::actions::AbilityInvocation {
                            aim: crate::actions::AbilityAim::ActorTarget {
                                target: SimActor(2),
                            },
                            weapon: crate::actions::WeaponSelection::Default,
                        },
                        supplies: Default::default(),
                    },
                },
            )
            .unwrap();
        let expected =
            serde_json::to_vec(&engine.game.preparation(SimActor(1)).unwrap().plan).unwrap();
        let game = engine.game.clone();
        let archive = serde_json::to_vec(&engine.archive).unwrap();
        let counts = tor_simulation::diagnostics::work_counts();
        for _ in 0..2 {
            let (bytes, _) = engine
                .profile_frozen_plan_json_encoding(actor)
                .unwrap()
                .unwrap();
            assert_eq!(bytes, expected.len() as u64);
        }
        assert_eq!(engine.game, game);
        assert_eq!(serde_json::to_vec(&engine.archive).unwrap(), archive);
        let after = tor_simulation::diagnostics::work_counts();
        assert_eq!(after.ability_plans, counts.ability_plans);
        assert_eq!(after.game_clones, counts.game_clones);
        assert_eq!(after.health_derivations, counts.health_derivations);
    }
}

#[cfg(test)]
#[path = "../test_support/public_plan_knowledge.rs"]
mod public_plan_knowledge_tests;

#[cfg(test)]
mod growth_capacity_tests {
    use super::*;
    #[test]
    fn growth_finalization_failure_preserves_operational_and_invariant_categories() {
        use tor_simulation::creatures::{BuildError, CreatureStateError};
        use tor_simulation::GameError;
        for error in [
            BuildError::Arithmetic,
            BuildError::Allocation,
            BuildError::TooManyHitDice,
        ] {
            for error in [
                GameError::Build(error),
                GameError::Creature(CreatureStateError::Build(error)),
                GameError::Growth(CreatureStateError::Build(error)),
            ] {
                let failure = Failure::simulation_failure(error).unwrap();
                assert_eq!(failure.code, ErrorCode::ResourceLimit);
                assert_eq!(failure.message, "Simulation resource capacity exhausted");
            }
        }
        for error in [
            BuildError::InvalidDefinition,
            BuildError::InvalidInteraction,
            BuildError::InvalidHealthRoot,
            BuildError::InvalidOwnership,
        ] {
            let failure =
                Failure::simulation_failure(GameError::Growth(CreatureStateError::Build(error)))
                    .unwrap();
            assert_eq!(failure.code, ErrorCode::InvalidAction);
            assert_eq!(
                failure.message,
                "Simulation state could not be updated; no changes were committed"
            );
            assert!(
                Failure::simulation_failure(GameError::Build(error)).is_none(),
                "ordinary rejected edits retain their existing action policy"
            );
        }
        for error in [
            GameError::Growth(CreatureStateError::BuildEpochExhausted),
            GameError::Creature(CreatureStateError::BuildEpochExhausted),
        ] {
            assert_eq!(
                Failure::simulation_failure(error).unwrap().code,
                ErrorCode::ResourceLimit
            );
        }
    }
    #[test]
    fn entry_identity_exhaustion_is_recoverable_without_disclosing_allocator_state() {
        let failure = Failure::simulation_failure(tor_simulation::GameError::IdentityExhausted)
            .expect("entry allocation capacity must not become an ordinary failed intention");
        assert_eq!(failure.code, ErrorCode::ResourceLimit);
        assert_eq!(failure.message, "Simulation resource capacity exhausted");
    }

    #[test]
    fn growth_capacity_error_is_generic_and_contains_no_private_actor_balance() {
        let error = tor_simulation::GameError::Creature(
            tor_simulation::creatures::CreatureStateError::Growth(
                tor_simulation::earned_growth::GrowthError::Arithmetic,
            ),
        );
        let failure = Failure::simulation_failure(error).unwrap();
        assert_eq!(failure.code, ErrorCode::ResourceLimit);
        assert_eq!(failure.message, "Simulation resource capacity exhausted");
        assert!(Failure::simulation_failure(tor_simulation::GameError::UnknownActor).is_none());
    }
}

#[cfg(test)]
mod earned_growth_identity_tests {
    use super::*;
    #[test]
    fn current_archive_and_rules_identity_reject_previous_versions_independently() {
        let engine = Engine::memory(Scenario::two_room(42)).unwrap();
        Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(engine.archive.version, ARCHIVE_VERSION);
        assert_eq!(engine.archive.ruleset, "interactions-v56");
        for previous_archive in [true, false] {
            let mut previous = engine.archive.clone();
            if previous_archive {
                previous.version = ARCHIVE_VERSION - 1;
            } else {
                previous.ruleset = "interactions-v42".into();
            }
            assert_eq!(
                Engine::replay(previous, None, None).unwrap_err().code,
                ErrorCode::InvalidArchive
            );
        }
    }
}

#[cfg(test)]
#[path = "../test_support/growth_capacity_transactions.rs"]
pub(crate) mod growth_capacity_transaction_tests;

#[cfg(test)]
mod playable_rogue_schema_tests {
    use super::*;

    #[test]
    fn current_archive_and_journal_require_explicit_activation_ledgers() {
        fn check<T: serde::de::DeserializeOwned + Serialize>(
            value: &T,
            fields: &[&str],
            ledger: &str,
        ) {
            let original = serde_json::to_value(value).unwrap();
            let bytes = serde_json::to_vec(&original).unwrap();
            crate::storage::codec::strict::<T>(&bytes).unwrap();
            assert_eq!(original.as_object().unwrap().len(), fields.len());
            let positional: Vec<_> = fields
                .iter()
                .map(|field| original.get(*field).unwrap().clone())
                .collect();
            assert!(
                crate::storage::codec::strict::<T>(&serde_json::to_vec(&positional).unwrap())
                    .is_err()
            );
            let mut missing = original.clone();
            assert!(missing.as_object_mut().unwrap().remove(ledger).is_some());
            assert!(
                crate::storage::codec::strict::<T>(&serde_json::to_vec(&missing).unwrap()).is_err()
            );
            let text = std::str::from_utf8(&bytes).unwrap();
            let duplicate = format!("{{\"{ledger}\":{},{}", original[ledger], &text[1..]);
            assert!(crate::storage::codec::strict::<T>(duplicate.as_bytes()).is_err());
        }

        let mut engine = crate::Engine::memory(crate::Scenario::two_room(42)).unwrap();
        engine
            .command(
                "test",
                "headless",
                ActorId(1),
                "activation-shape",
                &engine.branch().clone(),
                crate::journal::Command::Act {
                    expected_boundary: 0,
                    action: crate::journal::Action::Wait,
                },
            )
            .unwrap();
        check(
            &engine.archive,
            &[
                "view_salt",
                "wizard_game",
                "version",
                "ruleset",
                "ruleset_hash",
                "scenario",
                "branch",
                "records",
                "initial_activations",
                "initial_retrievals",
            ],
            "initial_activations",
        );
        assert_eq!(engine.archive.records.len(), 1);
        check(
            &engine.archive.records[0],
            &[
                "before_tick",
                "after_tick",
                "retrieval_before",
                "retrieval_after",
                "entry",
                "receipt",
                "action_resolution",
                "activations",
                "retrievals",
            ],
            "activations",
        );
    }
}

#[cfg(test)]
mod presentation_boundary_tests {
    use super::*;

    #[test]
    fn frozen_presentation_survives_actions_without_durable_display_counters() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let boundary = engine.presentation_boundary();
        let before = engine.state_at(&boundary, actor, 7).unwrap();
        let authoritative = engine.game.clone();
        assert_eq!(engine.causal_boundary(actor).unwrap(), 0);
        assert_eq!(engine.state_at(&boundary, actor, 7).unwrap(), before);
        assert_eq!(
            engine.game, authoritative,
            "materialization cannot acquire knowledge"
        );
        let branch = engine.branch().clone();
        let (_, profile) = engine
            .command_profiled(
                "test",
                "test",
                actor,
                "wait",
                &branch,
                Command::Act {
                    expected_boundary: 0,
                    action: Action::Wait,
                },
            )
            .unwrap();
        assert_eq!(
            profile.revision_comparisons, 0,
            "causal execution must not compare every actor's display"
        );
        assert_eq!(
            [
                profile.action_observer_boundaries,
                profile.action_event_receipts_recorded,
                profile.action_projection_validated,
                profile.action_navigation_reviewed,
            ],
            [0; 4],
            "an ordinary stationary wait has no affected observer cohort"
        );
        assert_eq!(engine.causal_boundary(actor).unwrap(), 1);
        assert_eq!(
            engine.state_at(&boundary, actor, 7).unwrap(),
            before,
            "an issued boundary never reads a later game"
        );
        assert_eq!(
            engine.state(actor).unwrap().revision,
            0,
            "internal semantic reads do not expose the causal ordinal"
        );
        let mut restored = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(restored.game, engine.game);
        assert_eq!(
            restored.causal_boundary(actor),
            engine.causal_boundary(actor)
        );
        // Private ordinal freshness is exact and independent of any materialization.
        assert_eq!(
            restored
                .command(
                    "test",
                    "test",
                    actor,
                    "stale",
                    &branch,
                    Command::Act {
                        expected_boundary: 0,
                        action: Action::Wait
                    }
                )
                .unwrap_err()
                .code,
            ErrorCode::StaleRevision
        );
        let mut moved = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor_count = moved.actors().len();
        let branch = moved.branch().clone();
        let (_, profile) = moved
            .command_profiled(
                "test",
                "test",
                actor,
                "move",
                &branch,
                Command::Act {
                    expected_boundary: 0,
                    action: Action::Move {
                        direction: Direction::East,
                    },
                },
            )
            .unwrap();
        assert_eq!(profile.revision_comparisons, 0);
        assert_eq!(
            [
                profile.action_observer_boundaries,
                profile.action_event_receipts_recorded,
                profile.action_projection_validated,
                profile.action_navigation_reviewed,
            ],
            [actor_count; 4],
            "each selected observer completes the same causal boundary duties"
        );
    }

    #[test]
    fn normalized_snapshot_annotation_replays_and_retries_without_token_lookup() {
        let mut engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let actor = ActorId(1);
        let boundary = engine.presentation_boundary();
        let snapshot = SnapshotId("issued-test-snapshot".into());
        let receipt = Receipt {
            user: "test".into(),
            frontend: "test".into(),
            actor,
            request_id: "note".into(),
            branch: engine.branch().clone(),
            command: Command::Annotate {
                anchor: Anchor::State {
                    snapshot: snapshot.clone(),
                },
                text: "remember this".into(),
                source: ClientSource::User,
                audience: Audience::Private,
                category: AnnotationCategory::Note,
            },
        };
        assert_eq!(
            engine.apply_command(&receipt, None).unwrap_err().code,
            ErrorCode::InvalidAnchor
        );
        let normalized = crate::journal::RecordedAnchor::State {
            snapshot,
            boundary: boundary.entry().cloned(),
        };
        let first = engine
            .command_with_annotation(&receipt, normalized)
            .unwrap();
        let retry = engine.apply_command(&receipt, None).unwrap();
        assert!(retry.duplicate);
        assert_eq!(retry.entry, first.entry);
        assert_eq!(engine.archive.records.len(), 1);
        let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
        assert_eq!(replayed.game, engine.game);
        assert_eq!(
            serde_json::to_value(&replayed.archive.records).unwrap(),
            serde_json::to_value(&engine.archive.records).unwrap()
        );
    }
}

#[cfg(test)]
mod semantic_capture_tests {
    use super::*;

    #[test]
    fn compact_event_inputs_match_cold_pictures_without_materializing_them() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/mob-arena");
        for scenario in [
            Scenario::two_room(42),
            crate::scenario_package::load(&root, 42, None, false).unwrap(),
        ] {
            let engine = Engine::memory(scenario).unwrap();
            for actor in engine.actors() {
                FULL_PICTURE_ADAPTATIONS.with(|count| count.set(0));
                let capture = engine.semantic_observation(actor).unwrap();
                assert_eq!(FULL_PICTURE_ADAPTATIONS.with(|count| count.get()), 0);
                assert!(engine
                    .semantic_picture_unchanged(&capture, &capture)
                    .unwrap());
                assert_eq!(FULL_PICTURE_ADAPTATIONS.with(|count| count.get()), 0);
                let full = engine.state(actor).unwrap();
                assert_eq!(
                    capture.facts,
                    tor_protocol::OutcomeObservation::from(&full.observation)
                );
                assert_eq!(engine.semantic_picture(&capture).unwrap(), full);
            }
        }
    }

    #[test]
    fn picture_proof_ignores_clock_but_falls_back_for_disclosed_changes() {
        let engine = Engine::memory(Scenario::two_room(42)).unwrap();
        let before = engine.semantic_observation(ActorId(1)).unwrap();
        let mut after = before.clone();
        Arc::make_mut(&mut after.view).observation.tick += 1;
        assert!(engine.semantic_picture_unchanged(&before, &after).unwrap());
        assert!(!tor_protocol::OutcomeFacts::picture_changed(
            &engine.semantic_picture(&before).unwrap().observation,
            &engine.semantic_picture(&after).unwrap().observation
        ));
        Arc::make_mut(&mut after.view).ready = !before.view.ready;
        assert!(!engine.semantic_picture_unchanged(&before, &after).unwrap());
        assert!(tor_protocol::OutcomeFacts::picture_changed(
            &engine.semantic_picture(&before).unwrap().observation,
            &engine.semantic_picture(&after).unwrap().observation
        ));
    }
}
