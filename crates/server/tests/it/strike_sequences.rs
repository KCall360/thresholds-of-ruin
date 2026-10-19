use serde_json::{json, Value};
use std::{io::Read, path::Path, sync::Arc};
use tor_protocol::{ActorId, CombatEventView};
use tor_server::{
    journal::{Action, Command, Direction, IntentionEndKind, JournalContent},
    scenario_package::{Manifest, Package, RegionDef},
    CommandResult, Engine, SavePolicy, Scenario,
};

fn scenario() -> Scenario {
    sequence_scenario(false)
}

fn sequence_scenario(gated: bool) -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(root.join("scenario.toml")).unwrap()).unwrap();
    let strike = |offset| {
        json!({"offset":offset,"unlock":null,"check":if gated {
            json!({"actor":"user","skill":{"kind":"selected_weapon"},
                "against":{"kind":"defense","defense":"physical"}})
        } else { json!(null) },
        "on_success":[{"kind":"damage","damage":{"kind":"components","components":[{
            "category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}}],
        "on_failure":[],"descriptors":[],"riders":[]})
    };
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    rules.abilities.push(serde_json::from_value(json!({"id":"triple_strike","name":"Triple Strike",
        "tags":[],"basic_attack":false,"cost":null,
        "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":"speed"},
        "reach":{"kind":"melee"},"strikes":{"kind":"authored","strikes":[strike(0),strike(100),strike(200)]},
        "interaction": "hostile_to_target", "ai":{"priority":100,"hostile":true,"minimum_resource_percent":0,"range_preference":"any"}})).unwrap());
    if gated {
        // One appended authored occurrence is copied to every natural row. Its
        // receipt must survive a restart after the second successful primary.
        rules.abilities.last_mut().unwrap().strikes = serde_json::from_value(json!({
            "kind":"natural_melee", "additional_riders":[{
                "trigger":"success", "condition":{"kind":"sequence_successful_hits","minimum":2},
                "check":null, "effects":[{"kind":"damage","damage":{
                    "kind":"components","components":[{"category":"keen","descriptor":null,
                        "amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}]}}],
                "descriptors":[]
            }]
        }))
        .unwrap();
    }
    manifest.creatures = serde_json::from_value(json!({"species":{"sequence_fighter":{
        "kind":"humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},"attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":if gated {100} else {0},"wind_up":100,"recovery":100,
            "strikes":if gated {vec![strike(0),strike(100),strike(200)]} else {vec![strike(0)]}},
        "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"triple_strike"}},
            {"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":200}}]
    }}})).unwrap();
    let creature = |name| {
        json!({"species":"sequence_fighter","name":name,"faction":"neutral",
        "binding":"intellect","starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":[{"source":"racial"}]})
    };
    manifest.characters[0].creature = Some(serde_json::from_value(creature("Escaper")).unwrap());
    let mut regions: Vec<RegionDef> = [1, 2]
        .into_iter()
        .map(|id| {
            toml::from_str(
                &std::fs::read_to_string(root.join(format!("regions/{id}.toml"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    regions[0].items.clear();
    regions[0].actors = vec![serde_json::from_value(
        json!({"id":2,"at":[2,1,0],"turn_ticks":if gated {100} else {150},
        "controller":"external","creature":creature("Sequence sentinel")}),
    )
    .unwrap()];
    let package =
        Package::from_parts(manifest, regions, &toml::to_string(&rules).unwrap()).unwrap();
    Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        ..Scenario::two_room(42)
    }
}

fn command(engine: &mut Engine, actor: ActorId, id: &str, command: Command) -> CommandResult {
    engine
        .command(
            "player",
            "test",
            actor,
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}
fn target_wait(engine: &mut Engine, id: &str) -> CommandResult {
    let revision = engine.revision(ActorId(2)).unwrap();
    command(
        engine,
        ActorId(2),
        id,
        Command::Act {
            expected_revision: revision,
            action: Action::Wait,
        },
    )
}
fn start_sequence(engine: &mut Engine) -> (CommandResult, Command) {
    let attack = Command::AdmitIntention {
        expected_revision: engine.revision(ActorId(1)).unwrap(),
        action: Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("triple_strike").unwrap(),
            invocation: tor_server::journal::AbilityInvocation {
                aim: tor_server::journal::AbilityAim::ActorTarget {
                    target: tor_simulation::ActorId(2),
                },
                weapon: tor_server::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    };
    let admitted = command(engine, ActorId(1), "sequence-admission", attack.clone());
    let started = engine.execute_next_intention().unwrap().unwrap();
    assert!(started.entry.intention_ends.is_empty());
    (admitted, attack)
}
fn start_prefix(engine: &mut Engine) -> (CommandResult, Command) {
    let (admitted, attack) = start_sequence(engine);
    let first = target_wait(engine, "first-target-wait");
    assert!(!first
        .entry
        .intention_ends
        .iter()
        .any(|end| end.actor == ActorId(1)));
    let own = engine.state(ActorId(1)).unwrap();
    let progress = own
        .observation
        .combat
        .as_ref()
        .unwrap()
        .sequence
        .as_ref()
        .unwrap();
    assert_eq!((progress.completed, progress.total), (1, 3));
    assert_eq!(progress.next_strike_remaining, 50);
    assert_eq!(progress.cancel_recovery_remaining, 250);
    (admitted, attack)
}

#[test]
fn ordered_strikes_normal_package_publishes_prefix_privately_and_one_terminal_completion() {
    let mut engine = Engine::memory(scenario()).unwrap();
    let before = engine
        .state(ActorId(2))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .hp;
    let (admitted, _) = start_prefix(&mut engine);
    let target = engine.state(ActorId(2)).unwrap();
    let combat = target.observation.combat.as_ref().unwrap();
    assert!(combat.sequence.is_none());
    assert_eq!(combat.hp, before - 1);
    assert!(combat.events.iter().any(|event| matches!(
        event,
        CombatEventView::Ability {
            strike: Some(0),
            ..
        }
    )));
    let visible = serde_json::to_value(combat).unwrap();
    for event in visible["events"].as_array().unwrap() {
        for private in ["total", "check", "tokens", "plan", "intention"] {
            assert!(event.get(private).is_none(), "leaked {private}");
        }
    }
    let final_wait = target_wait(&mut engine, "final-target-wait");
    let JournalContent::IntentionAdmitted { intention, .. } = admitted.entry.content else {
        panic!("admission");
    };
    assert_eq!(
        final_wait
            .entry
            .intention_ends
            .iter()
            .filter(|end| end.intention == intention && end.kind == IntentionEndKind::Resolved)
            .count(),
        1
    );
    assert!(engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .sequence
        .is_none());
    assert_eq!(
        engine
            .state(ActorId(2))
            .unwrap()
            .observation
            .combat
            .unwrap()
            .hp,
        before - 3
    );
}

#[test]
fn ordered_strikes_cancel_and_escape_are_atomic_exact_retryable_transactions() {
    for step in [None, Some(Direction::West)] {
        let mut engine = Engine::memory(scenario()).unwrap();
        let (admitted, _) = start_prefix(&mut engine);
        let before = engine.state(ActorId(1)).unwrap();
        let target = engine.state(ActorId(2)).unwrap();
        let counts = engine.profile_counts();
        let rejected = Command::CancelIntention {
            expected_revision: before.revision,
            admission: admitted.entry.id.clone(),
            step: Some(Direction::East),
        };
        assert!(engine
            .command(
                "player",
                "test",
                ActorId(1),
                "blocked-escape",
                &engine.branch().clone(),
                rejected
            )
            .is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), before);
        assert_eq!(engine.state(ActorId(2)).unwrap(), target);
        assert_eq!(engine.profile_counts(), counts);
        let cancel = Command::CancelIntention {
            expected_revision: before.revision,
            admission: admitted.entry.id.clone(),
            step,
        };
        let accepted = command(&mut engine, ActorId(1), "cancel-sequence", cancel.clone());
        assert_eq!(accepted.entry.intention_ends.len(), 1);
        assert_eq!(
            accepted.entry.intention_ends[0].kind,
            IntentionEndKind::Cancelled
        );
        let after = engine.state(ActorId(1)).unwrap();
        let combat = after.observation.combat.as_ref().unwrap();
        assert!(combat.sequence.is_none());
        assert_eq!(
            combat.recovery_remaining,
            if step.is_some() { 350 } else { 250 }
        );
        let replayed = command(&mut engine, ActorId(1), "cancel-sequence", cancel);
        assert!(replayed.duplicate);
        assert_eq!(replayed.entry, accepted.entry);
        assert_eq!(engine.state(ActorId(1)).unwrap(), after);
        let changed = Command::CancelIntention {
            expected_revision: before.revision,
            admission: admitted.entry.id,
            step: Some(Direction::North),
        };
        assert!(engine
            .command(
                "player",
                "test",
                ActorId(1),
                "cancel-sequence",
                &engine.branch().clone(),
                changed
            )
            .is_err());
        assert_eq!(engine.state(ActorId(1)).unwrap(), after);
    }
}

// Read only our flushed interval-one fixture. Normal Engine loading remains
// the strict save-admission authority; this diagnostic exposes private witnesses
// without exporting a mutation or snapshot API just for the test.
fn saved_checkpoint(path: &Path) -> Value {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let (sequence, payload): (i64, Vec<u8>) = db
        .query_row(
            "SELECT sequence, payload FROM checkpoint WHERE slot=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(sequence >= 0, "persisted sequence must be nonnegative");
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM journal WHERE sequence > ?1",
            [sequence],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "the checkpoint must cover the complete flushed tail"
    );
    assert!((12..=64 * 1024 * 1024).contains(&payload.len()));
    assert_eq!(&payload[..8], b"TORC\x01\x00\x00\x00");
    let declared = u32::from_le_bytes(payload[payload.len() - 4..].try_into().unwrap());
    assert!(declared <= 64 * 1024 * 1024);
    let mut decoder = flate2::read::ZlibDecoder::new(&payload[8..payload.len() - 4]);
    let mut decoded = Vec::new();
    (&mut decoder)
        .take(u64::from(declared) + 1)
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded.len(), declared as usize);
    assert_eq!(decoder.total_in(), (payload.len() - 12) as u64);
    serde_json::from_slice(&decoded).unwrap()
}

fn saved_preparation(checkpoint: &Value) -> &Value {
    let actors = checkpoint["game"]["actors"].as_u64().unwrap() as usize;
    &checkpoint["shared"]["actors"][actors]["1"]["pending"]
}

fn target_hp(engine: &Engine) -> u64 {
    engine
        .state(ActorId(2))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .hp
}

#[test]
fn ordered_strikes_saved_prefix_resumes_without_replaying_paid_strikes_or_admissions() {
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sequence.db");
        let policy = SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut engine =
            Engine::open_with_policy(&path, sequence_scenario(true), policy.clone()).unwrap();
        let target_before = target_hp(&engine);
        let (admitted, attack) = start_sequence(&mut engine);
        let JournalContent::IntentionAdmitted { intention, .. } = admitted.entry.content else {
            panic!("admission");
        };
        let mut paid_damage = 0;
        // Restart once with a qualifying hit but no receipt, then again after
        // the threshold paid the rolled rider. Both prefixes still own a tail.
        for completed in 1..=2 {
            let work = tor_simulation::diagnostics::work_counts();
            let advanced = target_wait(&mut engine, &format!("prefix-{completed}"));
            assert!(advanced.entry.intention_ends.is_empty());
            let after = tor_simulation::diagnostics::work_counts();
            assert_eq!(after.strike_resolutions - work.strike_resolutions, 1);
            assert_eq!(
                after.rider_resolutions - work.rider_resolutions,
                usize::from(completed == 2)
            );
            let damage = target_before - target_hp(&engine);
            if completed == 1 {
                assert_eq!(damage, 1, "no rider before the second qualifying hit");
            } else {
                assert!(
                    (3..=8).contains(&damage),
                    "two primaries and exactly one 1d6 rider"
                );
            }
            paid_damage = damage;
            let before = [
                engine.state(ActorId(1)).unwrap(),
                engine.state(ActorId(2)).unwrap(),
            ];
            let history = engine.history(ActorId(1), "player", None, 100).unwrap();
            let counts = engine.profile_counts();
            let sequence = before[0]
                .observation
                .combat
                .as_ref()
                .unwrap()
                .sequence
                .clone()
                .unwrap();
            assert_eq!((sequence.completed, sequence.total), (completed, 3));
            assert_eq!(sequence.next_strike_remaining, 100);
            assert_eq!(
                sequence.cancel_recovery_remaining,
                (4 - u64::from(completed)) * 100
            );
            engine.flush().unwrap();
            if interval == 1 {
                let checkpoint = saved_checkpoint(&path);
                assert_eq!(
                    saved_preparation(&checkpoint)["progress"],
                    json!({
                        "kind":"ability", "next_strike":completed,
                        "successful_primaries":if completed == 1 {1} else {3},
                        "attempted_riders":[u64::from(completed == 2), 0], "linked_hold":null
                    })
                );
            }
            drop(engine);
            // Only pinned save content is available; the replacement scenario
            // has no sequence ability, species, or gate source.
            engine =
                Engine::open_with_policy(&path, Scenario::two_room(99), policy.clone()).unwrap();
            if interval == 1 {
                assert_eq!(engine.recovery_profile().records_replayed, 0);
            } else {
                assert!(engine.recovery_profile().records_replayed > 0);
            }
            for (id, state) in [ActorId(1), ActorId(2)].into_iter().zip(&before) {
                assert_eq!(&engine.state(id).unwrap(), state);
            }
            assert_eq!(
                engine.history(ActorId(1), "player", None, 100).unwrap(),
                history
            );
            assert_eq!(engine.profile_counts(), counts);
            assert!(
                before[0]
                    .observation
                    .combat
                    .as_ref()
                    .unwrap()
                    .preparation_active
            );
            // Service startup uses this ordinary durable pause. Loading itself
            // must not silently rewrite the paid preparation or its hit history.
            let pause = engine.pause_preparation(ActorId(1)).unwrap().unwrap();
            assert!(pause.entry.intention_ends.is_empty());
            let restored = engine.state(ActorId(1)).unwrap();
            let combat = restored.observation.combat.as_ref().unwrap();
            assert_eq!(combat.sequence.as_ref(), Some(&sequence));
            assert!(!combat.preparation_active);
            assert_eq!(target_hp(&engine), target_before - paid_damage);
            let accepted_sequence = engine.save_status().accepted_sequence;
            let counts = engine.profile_counts();
            let duplicate = command(
                &mut engine,
                ActorId(1),
                "sequence-admission",
                attack.clone(),
            );
            assert!(duplicate.duplicate);
            assert_eq!(duplicate.entry, admitted.entry);
            assert_eq!(engine.save_status().accepted_sequence, accepted_sequence);
            assert_eq!(engine.profile_counts(), counts);
            assert_eq!(engine.state(ActorId(1)).unwrap(), restored);
            assert_eq!(
                engine.history(ActorId(1), "player", None, 100).unwrap(),
                history
            );
            command(
                &mut engine,
                ActorId(1),
                &format!("resume-{completed}"),
                Command::ResumeIntention {
                    expected_revision: restored.revision,
                    admission: admitted.entry.id.clone(),
                },
            );
            let resumed = engine.execute_next_intention().unwrap().unwrap();
            assert!(resumed.entry.intention_ends.is_empty());
            assert_eq!(target_hp(&engine), target_before - paid_damage);
        }
        let work = tor_simulation::diagnostics::work_counts();
        let ended = target_wait(&mut engine, "final-target-wait");
        let after = tor_simulation::diagnostics::work_counts();
        assert_eq!(after.strike_resolutions - work.strike_resolutions, 1);
        assert_eq!(
            after.rider_resolutions - work.rider_resolutions,
            0,
            "saved once receipt prevents a third-hit replay"
        );
        assert_eq!(
            ended
                .entry
                .intention_ends
                .iter()
                .filter(|end| end.intention == intention && end.kind == IntentionEndKind::Resolved)
                .count(),
            1
        );
        assert_eq!(target_hp(&engine), target_before - paid_damage - 1);
        assert!(engine
            .state(ActorId(1))
            .unwrap()
            .observation
            .combat
            .unwrap()
            .sequence
            .is_none());
        let history = engine.history(ActorId(1), "player", None, 100).unwrap();
        assert_eq!(
            history.entries.len(),
            3,
            "one initial start and exactly two explicit continuations"
        );
        let final_states = [
            engine.state(ActorId(1)).unwrap(),
            engine.state(ActorId(2)).unwrap(),
        ];
        engine.flush().unwrap();
        drop(engine);
        let restored = Engine::open(&path, Scenario::two_room(99)).unwrap();
        assert_eq!(
            restored.history(ActorId(1), "player", None, 100).unwrap(),
            history
        );
        for (id, state) in [ActorId(1), ActorId(2)].into_iter().zip(final_states) {
            assert_eq!(restored.state(id).unwrap(), state);
        }
    }
}

#[test]
fn failed_sequence_rider_journal_admission_preserves_prefix_and_replays_the_same_rolls() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sequence.db");
    let reference_path = directory.path().join("reference.db");
    let policy = SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut engine =
        Engine::open_with_policy(&path, sequence_scenario(true), policy.clone()).unwrap();
    let target_before = target_hp(&engine);
    start_sequence(&mut engine);
    target_wait(&mut engine, "first-target-wait");
    assert_eq!(target_hp(&engine), target_before - 1);
    engine.flush().unwrap();
    let saved = saved_checkpoint(&path);
    let archive = tor_server::inspect_save(&path).unwrap();
    assert_eq!(
        saved_preparation(&saved)["progress"],
        json!({
            "kind":"ability", "next_strike":1, "successful_primaries":1, "attempted_riders":[0, 0],
            "linked_hold":null
        })
    );
    drop(engine);
    std::fs::copy(&path, &reference_path).unwrap();
    let mut reference =
        Engine::open_with_policy(&reference_path, Scenario::two_room(99), policy.clone()).unwrap();
    let mut rejected = Engine::open_with_policy(
        &path,
        Scenario::two_room(99),
        SavePolicy {
            max_pending_bytes: 1,
            ..policy.clone()
        },
    )
    .unwrap();
    let before = [
        rejected.state(ActorId(1)).unwrap(),
        rejected.state(ActorId(2)).unwrap(),
    ];
    let history = [
        rejected.history(ActorId(1), "player", None, 100).unwrap(),
        rejected.history(ActorId(2), "player", None, 100).unwrap(),
    ];
    let counts = rejected.profile_counts();
    let sequence = rejected.save_status().accepted_sequence;
    let next = Command::Act {
        expected_revision: before[1].revision,
        action: Action::Wait,
    };
    for _ in 0..2 {
        let work = tor_simulation::diagnostics::work_counts();
        let failure = rejected
            .command(
                "player",
                "test",
                ActorId(2),
                "threshold-target-wait",
                &rejected.branch().clone(),
                next.clone(),
            )
            .unwrap_err();
        assert_eq!(failure.code, tor_protocol::ErrorCode::StorageFailure);
        assert_eq!(
            failure.message,
            "Save queue is full; wait for saving to finish before retrying"
        );
        let after = tor_simulation::diagnostics::work_counts();
        // This is a late refusal: the candidate earned the second hit, attempted
        // the rider, consumed its dice and applied damage before storage failed.
        assert_eq!(after.strike_resolutions - work.strike_resolutions, 1);
        assert_eq!(after.rider_resolutions - work.rider_resolutions, 1);
        assert_eq!(rejected.profile_counts(), counts);
        assert_eq!(rejected.save_status().accepted_sequence, sequence);
        assert_eq!(rejected.save_status().durable_sequence, sequence);
        rejected.flush().unwrap();
        assert_eq!(
            saved_checkpoint(&path),
            saved,
            "failed admission cannot persist hit bits, receipt, effects or RNG"
        );
        assert_eq!(tor_server::inspect_save(&path).unwrap(), archive);
        for (index, id) in [ActorId(1), ActorId(2)].into_iter().enumerate() {
            assert_eq!(rejected.state(id).unwrap(), before[index]);
            assert_eq!(
                rejected.history(id, "player", None, 100).unwrap(),
                history[index]
            );
        }
    }
    rejected.flush().unwrap();
    drop(rejected);
    let mut recovered = Engine::open_with_policy(&path, Scenario::two_room(99), policy).unwrap();
    for (index, id) in [ActorId(1), ActorId(2)].into_iter().enumerate() {
        assert_eq!(recovered.state(id).unwrap(), before[index]);
        assert_eq!(
            recovered.history(id, "player", None, 100).unwrap(),
            history[index]
        );
    }
    // Exact saved private state against the untouched reference proves the RNG
    // and receipt continuation, not merely a coincidentally equal damage roll.
    // This does not expose the rejected Engine's private in-memory RNG.
    for (request, riders) in [("threshold-target-wait", 1), ("final-target-wait", 0)] {
        let expected = target_wait(&mut reference, request);
        let work = tor_simulation::diagnostics::work_counts();
        let actual = target_wait(&mut recovered, request);
        let after = tor_simulation::diagnostics::work_counts();
        assert_eq!(after.strike_resolutions - work.strike_resolutions, 1);
        assert_eq!(after.rider_resolutions - work.rider_resolutions, riders);
        assert_eq!(actual.entry.content, expected.entry.content);
        assert_eq!(actual.entry.tick, expected.entry.tick);
        assert_eq!(actual.entry.intention_ends, expected.entry.intention_ends);
        for id in [ActorId(1), ActorId(2)] {
            assert_eq!(recovered.state(id).unwrap(), reference.state(id).unwrap());
        }
        assert_eq!(recovered.profile_counts(), reference.profile_counts());
        assert_eq!(
            recovered.save_status().accepted_sequence,
            reference.save_status().accepted_sequence
        );
        recovered.flush().unwrap();
        reference.flush().unwrap();
        let actual = saved_checkpoint(&path);
        let expected = saved_checkpoint(&reference_path);
        assert_eq!(actual["game"]["rules_rng"], expected["game"]["rules_rng"]);
        assert_ne!(
            actual["game"]["rules_rng"], saved["game"]["rules_rng"],
            "the successful continuation consumed combat dice"
        );
        for field in ["game", "shared", "revisions"] {
            assert_eq!(
                actual[field], expected[field],
                "exact saved {field}, including private actors, events and RNG"
            );
        }
        if riders == 1 {
            assert_eq!(
                saved_preparation(&actual)["progress"],
                json!({
                    "kind":"ability", "next_strike":2, "successful_primaries":3, "attempted_riders":[1, 0],
                    "linked_hold":null
                })
            );
        } else {
            assert!(saved_preparation(&actual).is_null());
        }
    }
    assert!((4..=9).contains(&(target_before - target_hp(&recovered))));
    recovered.flush().unwrap();
}
