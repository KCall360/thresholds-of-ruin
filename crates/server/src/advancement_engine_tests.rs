//! Ordinary Engine/Store tests below the separately tested Service authorization.
use super::*;
use crate::{
    advancement_test_support::{replacement_scenario, scenario},
    journal::{Action, Command, JournalContent, SupplySelection},
    SavePolicy, Scenario,
};

const OWNER: ActorId = ActorId(1);
fn context(engine: &Engine) -> p::ReplyContext {
    p::ReplyContext {
        snapshot: tor_protocol::SnapshotId("fixture".into()),
        input: p::InputContext {
            stream: p::StreamContext {
                stream: p::StreamId("owner-test-stream".into()),
                epoch: 0,
            },
            readiness_revision: 0,
        },
        actor: OWNER,
        branch: engine.branch().clone(),
        cursor: p::StreamCursor {
            sequence: 0,
            tick: engine.game.tick(),
        },
        revision: engine.causal_boundary(OWNER).unwrap(),
    }
}
fn basis(engine: &Engine) -> s::player_advancement::AdvancementBasis {
    adapter::basis(&engine.advancement_basis(&context(engine)).unwrap()).unwrap()
}
fn purchase(engine: &Engine) -> Action {
    Action::Purchase {
        basis: Box::new(basis(engine)),
        purchase: s::player_advancement::PurchaseOperation {
            owner: tor_simulation::HdIndex(0),
            pool: s::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: s::player_advancement::PurchaseChoice::SkillRank {
                skill: s::attributes::Skill::Perception,
            },
        },
    }
}
fn replacement(engine: &Engine) -> Action {
    Action::Respec {
        basis: Box::new(basis(engine)),
        replacement: s::player_respec::ReplacementOperation {
            source: s::player_respec::ReplacementSource {
                owner: tor_simulation::HdIndex(0),
                index: 0,
            },
            replacement: None,
        },
        supplies: SupplySelection::Carried,
    }
}
fn submit(
    engine: &mut Engine,
    actor: ActorId,
    request: &str,
    command: Command,
) -> crate::CommandResult {
    engine
        .command(
            "owner",
            "owner-test",
            actor,
            request,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}
fn admission(engine: &Engine, action: Action) -> Command {
    Command::AdmitIntention {
        expected_boundary: engine.causal_boundary(OWNER).unwrap(),
        action,
    }
}
fn wait_request(engine: &Engine) -> (ActorId, Command) {
    let actor = engine.next_actor().expect("ordinary live actor");
    (
        actor,
        Command::Act {
            expected_boundary: engine.causal_boundary(actor).unwrap(),
            action: Action::Wait,
        },
    )
}
fn step(engine: &mut Engine) {
    let (actor, command) = wait_request(engine);
    submit(engine, actor, &uuid::Uuid::new_v4().to_string(), command);
}
fn ration(engine: &Engine) -> u64 {
    engine
        .game
        .observe(s::ActorId(1))
        .unwrap()
        .inventory
        .iter()
        .filter(|item| item.id == s::ItemId(10))
        .map(|item| item.quantity)
        .sum()
}
fn query(engine: &Engine, query: p::AdvancementQuery) -> p::AdvancementPage {
    let context = context(engine);
    engine
        .advancement_query(
            OWNER,
            engine.branch(),
            context.revision,
            &query,
            &PageBudget {
                context: &context,
                request_id: "owner-test",
                limit: p::MAX_ADVANCEMENT_PAGE_BYTES,
            },
        )
        .unwrap()
}
fn overview(engine: &Engine) -> p::AdvancementOverview {
    let page = query(engine, p::AdvancementQuery::Overview {});
    let p::AdvancementEntry::Overview { value } = page.entries.into_iter().next().unwrap() else {
        panic!("overview")
    };
    *value
}

#[test]
fn owner_host_all_preview_sections_are_read_only_and_exact_while_busy() {
    let mut engine = Engine::memory(replacement_scenario(false)).unwrap();
    let original = engine.game.clone();
    let profile = engine.profile_counts();
    let operation = p::ReplacementOperation {
        source: p::ReplacementSource { owner: 0, index: 0 },
        replacement: None,
    };
    let proposal = p::AdvancementProposal::Replacement {
        replacement: operation,
    };
    let b = engine.advancement_basis(&context(&engine)).unwrap();
    let mut totals = Vec::new();
    for section in [
        p::AdvancementPreviewSection::Summary,
        p::AdvancementPreviewSection::Removed,
        p::AdvancementPreviewSection::Refunds,
        p::AdvancementPreviewSection::Balances,
        p::AdvancementPreviewSection::Skills,
        p::AdvancementPreviewSection::Talents,
        p::AdvancementPreviewSection::Expertises,
        p::AdvancementPreviewSection::RemovedConditions,
    ] {
        let q = p::AdvancementQuery::Preview {
            basis: b.clone(),
            proposal: proposal.clone(),
            section,
            offset: 0,
        };
        let page = query(&engine, q.clone());
        assert_eq!(page.query, q);
        totals.push(page.total);
    }
    assert_eq!(totals, [1, 1, 1, 2, 20, 0, 0, 0]);
    assert_eq!(
        engine.game, original,
        "includes RNG, holds, actor-active clocks and knowledge"
    );
    assert_eq!(engine.profile_counts(), profile);
    let command = admission(&engine, replacement(&engine));
    submit(&mut engine, OWNER, "respec", command);
    engine.execute_next_intention().unwrap().unwrap();
    let active = overview(&engine);
    assert!(!active.ready);
    assert_eq!(active.respec.as_ref().unwrap().remaining_ticks, 600);
    let busy = engine.game.clone();
    let page = query(
        &engine,
        p::AdvancementQuery::Preview {
            basis: b,
            proposal,
            section: p::AdvancementPreviewSection::Summary,
            offset: 0,
        },
    );
    assert_eq!(page.total, 1);
    assert_eq!(
        engine.game, busy,
        "a busy owner still has free preview reads"
    );
}

#[test]
fn owner_host_queued_purchase_reopens_and_retains_original_operands_and_receipt() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("purchase.db");
        let fixture = scenario();
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let action = purchase(&engine);
        let before_epoch = basis(&engine).build_epoch;
        let command = admission(&engine, action.clone());
        let accepted = submit(&mut engine, OWNER, "purchase", command.clone());
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let started = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(&started.entry.content, JournalContent::IntentionStarted { action: saved, .. } if saved == &action)
        );
        assert_eq!(basis(&engine).build_epoch.get(), before_epoch.get() + 1);
        assert_eq!(
            engine
                .game
                .creature(s::ActorId(1))
                .unwrap()
                .derived()
                .skills
                .get(s::attributes::Skill::Perception),
            1
        );
        let completed = engine.game.clone();
        let duplicate = submit(&mut engine, OWNER, "purchase", command.clone());
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, accepted.entry);
        assert_eq!(engine.game, completed);
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
        assert_eq!(engine.game, completed);
        assert!(submit(&mut engine, OWNER, "purchase", command).duplicate);
        assert!(engine.execute_next_intention().unwrap().is_none());
    }
}

#[test]
fn owner_host_store_refusal_preserves_admission_and_reserved_start() {
    for checkpoint_interval in [1, 4096] {
        for operation in 0..3 {
            let respec = operation == 1;
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("start.db");
            let fixture = if respec {
                replacement_scenario(false)
            } else {
                scenario()
            };
            let policy = SavePolicy {
                checkpoint_interval,
                ..Default::default()
            };
            let engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
            let action = if respec {
                replacement(&engine)
            } else if operation == 2 {
                Action::ChooseGrowthSource {
                    basis: Box::new(basis(&engine)),
                    source: s::earned_growth::GrowthSource::Racial {},
                }
            } else {
                purchase(&engine)
            };
            let command = admission(&engine, action);
            engine.flush().unwrap();
            drop(engine);
            let refusing = SavePolicy {
                max_pending_bytes: 1,
                ..policy.clone()
            };
            let mut engine =
                Engine::open_with_policy(&path, fixture.clone(), refusing.clone()).unwrap();
            let original = engine.game.clone();
            let saved = crate::inspect_save(&path).unwrap();
            let counts = engine.profile_counts();
            let sequence = engine.save_status().accepted_sequence;
            let failure = engine
                .command(
                    "owner",
                    "owner-test",
                    OWNER,
                    "admit",
                    &engine.branch().clone(),
                    command.clone(),
                )
                .unwrap_err();
            assert_eq!(failure.code, p::ErrorCode::StorageFailure);
            assert_eq!(engine.game, original);
            assert_eq!(engine.profile_counts(), counts);
            assert_eq!(engine.save_status().accepted_sequence, sequence);
            assert_eq!(crate::inspect_save(&path).unwrap(), saved);
            drop(engine);
            let mut engine =
                Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
            let accepted = submit(&mut engine, OWNER, "admit", command.clone());
            engine.flush().unwrap();
            drop(engine);
            let mut engine = Engine::open_with_policy(&path, fixture.clone(), refusing).unwrap();
            let original = engine.game.clone();
            let saved = crate::inspect_save(&path).unwrap();
            let counts = engine.profile_counts();
            for _ in 0..2 {
                assert_eq!(
                    engine.execute_next_intention().unwrap_err().code,
                    p::ErrorCode::StorageFailure
                );
                assert_eq!(engine.game, original);
                assert_eq!(engine.profile_counts(), counts);
                assert_eq!(crate::inspect_save(&path).unwrap(), saved);
                let duplicate = submit(&mut engine, OWNER, "admit", command.clone());
                assert!(duplicate.duplicate);
                assert_eq!(duplicate.entry, accepted.entry);
            }
            drop(engine);
            let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
            let started = engine.execute_next_intention().unwrap().unwrap();
            assert!(matches!(
                started.entry.content,
                JournalContent::IntentionStarted { .. }
            ));
            if respec {
                assert_eq!(ration(&engine), 1);
                assert!(overview(&engine).respec.is_some());
            } else {
                assert_eq!(basis(&engine).build_epoch.get(), 2);
            }
        }
    }
}

fn finish_preparation(engine: &mut Engine) {
    for _ in 0..100 {
        if engine.game.preparation(s::ActorId(1)).is_none() {
            return;
        }
        step(engine);
    }
    panic!("ordinary preparation must finish");
}
fn prepare_lethal_injury(engine: &mut Engine) {
    let slot = engine
        .game
        .anatomy(s::ActorId(1))
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "ring")
        .unwrap();
    let equipment = admission(
        engine,
        Action::Equip {
            item: 12,
            slot: u16::try_from(slot).unwrap(),
        },
    );
    submit(engine, OWNER, "equip", equipment);
    engine.execute_next_intention().unwrap().unwrap();
    finish_preparation(engine);
    assert!(!engine.game.equipment(s::ActorId(1)).unwrap().is_empty());
    for _ in 0..100 {
        if engine.game.player_advancement_ready(s::ActorId(1)) {
            break;
        }
        step(engine);
    }
    assert!(engine.game.player_advancement_ready(s::ActorId(1)));
    let before = engine.game.health(s::ActorId(1)).unwrap();
    assert!(
        before.0 > 100,
        "ordinary dose must be nonlethal: {before:?}"
    );
    let command = admission(engine, Action::Drink { item: 11 });
    submit(engine, OWNER, "injury", command);
    engine.execute_next_intention().unwrap().unwrap();
    finish_preparation(engine);
    for _ in 0..100 {
        if engine.game.player_advancement_ready(s::ActorId(1))
            && engine
                .game
                .creature(s::ActorId(1))
                .unwrap()
                .received_health_loss_remaining()
                == 0
        {
            break;
        }
        step(engine);
    }
    assert!(engine.game.player_advancement_ready(s::ActorId(1)));
    assert_eq!(
        engine
            .game
            .creature(s::ActorId(1))
            .unwrap()
            .received_health_loss_remaining(),
        0
    );
    assert!(engine.game.health(s::ActorId(1)).unwrap().0 > 0);
    assert!(engine
        .game
        .observe(s::ActorId(1))
        .unwrap()
        .inventory
        .iter()
        .all(|item| item.id != s::ItemId(11)));
}

#[test]
fn owner_host_store_refusal_after_replacement_spend_rolls_back_and_replays_once() {
    for checkpoint_interval in [1, 4096] {
        for lethal in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("completion.db");
            let fixture = replacement_scenario(lethal);
            let policy = SavePolicy {
                checkpoint_interval,
                ..Default::default()
            };
            let mut engine =
                Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
            if lethal {
                prepare_lethal_injury(&mut engine);
            }
            let action = replacement(&engine);
            let Action::Respec {
                basis: original_basis,
                replacement,
                ..
            } = &action
            else {
                unreachable!()
            };
            let preview = engine
                .game
                .preview_player_replacement(s::ActorId(1), original_basis, replacement)
                .unwrap();
            assert_eq!(
                preview.runtime().health.dead(),
                lethal,
                "positive endpoint control before admission"
            );
            let original_epoch = original_basis.build_epoch;
            let command = admission(&engine, action.clone());
            let accepted = submit(&mut engine, OWNER, "respec", command.clone());
            let started = engine.execute_next_intention().unwrap().unwrap();
            assert!(
                matches!(&started.entry.content, JournalContent::IntentionStarted { action: stored, .. } if stored == &action)
            );
            assert_eq!(ration(&engine), 1);
            for _ in 0..100 {
                let timing = engine.game.preparation_timing(s::ActorId(1)).unwrap();
                if timing.preparation_remaining.unwrap() <= 100 {
                    break;
                }
                step(&mut engine);
            }
            let active = overview(&engine).respec.unwrap();
            assert!(active.active);
            assert!(active.remaining_ticks > 0 && active.remaining_ticks <= 100);
            assert_eq!(active.basis, adapter::wire_basis(original_basis));
            assert_eq!(ration(&engine), 1);
            let preview = engine
                .game
                .preview_player_replacement(s::ActorId(1), original_basis, replacement)
                .unwrap();
            assert_eq!(
                preview.runtime().health.dead(),
                lethal,
                "the saved boundary still reaches the intended endpoint"
            );
            let (actor, completion) = wait_request(&engine);
            assert_eq!(
                actor,
                ActorId(2),
                "ordinary ally Wait reaches timed completion"
            );
            engine.flush().unwrap();
            let saved = crate::inspect_save(&path).unwrap();
            drop(engine);
            let mut engine = Engine::open_with_policy(
                &path,
                fixture.clone(),
                SavePolicy {
                    max_pending_bytes: 1,
                    ..policy.clone()
                },
            )
            .unwrap();
            let before = engine.game.clone();
            let before_states = [OWNER, ActorId(2)].map(|actor| engine.state(actor).unwrap());
            let histories =
                [OWNER, ActorId(2)].map(|actor| engine.history(actor, "owner", None, 50).unwrap());
            let counts = engine.profile_counts();
            let sequence = engine.save_status().accepted_sequence;
            for _ in 0..2 {
                let error = engine
                    .command(
                        "owner",
                        "owner-test",
                        actor,
                        "complete",
                        &engine.branch().clone(),
                        completion.clone(),
                    )
                    .unwrap_err();
                assert_eq!(error.code, p::ErrorCode::StorageFailure);
                assert_eq!(
                    engine.game, before,
                    "includes last ration, exact hold lineage, corpse allocator and events"
                );
                assert_eq!(
                    [OWNER, ActorId(2)].map(|actor| engine.state(actor).unwrap()),
                    before_states
                );
                assert_eq!(
                    [OWNER, ActorId(2)]
                        .map(|actor| engine.history(actor, "owner", None, 50).unwrap()),
                    histories
                );
                assert_eq!(engine.profile_counts(), counts);
                assert_eq!(engine.save_status().accepted_sequence, sequence);
                assert_eq!(engine.save_status().durable_sequence, sequence);
                assert_eq!(crate::inspect_save(&path).unwrap(), saved);
                let duplicate = submit(&mut engine, OWNER, "respec", command.clone());
                assert!(duplicate.duplicate);
                assert_eq!(duplicate.entry, accepted.entry);
            }
            drop(engine);
            // Identical ordinary command on the saved source reaches successful
            // Spend+replacement. This is the positive twin of the Store refusal.
            let mut engine =
                Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
            let completed = submit(&mut engine, actor, "complete", completion.clone());
            assert_eq!(ration(&engine), 0);
            assert_eq!(basis(&engine).build_epoch.get(), original_epoch.get() + 1);
            let creature = engine.game.creature(s::ActorId(1)).unwrap();
            assert_eq!(creature.health().dead(), lethal);
            assert!(creature.build().choices()[0].purchases.is_empty());
            let observation = engine.game.observe(s::ActorId(1)).unwrap();
            assert!(observation
                .inventory
                .iter()
                .map(|item| item.id)
                .chain(observation.ground_items.iter().map(|item| item.id))
                .all(|id| id != s::ItemId(10)));
            assert_eq!(
                observation
                    .ground_items
                    .iter()
                    .filter(|item| item.class == s::ItemClass::Corpse)
                    .count(),
                usize::from(lethal)
            );
            if lethal {
                assert!(engine.game.equipment(s::ActorId(1)).unwrap().is_empty());
                assert_eq!(
                    observation
                        .ground_items
                        .iter()
                        .filter(|item| item.id == s::ItemId(12))
                        .count(),
                    1
                );
                assert!(observation
                    .ground_items
                    .iter()
                    .all(|item| item.id != s::ItemId(11)));
                assert_eq!(
                    creature.health().maximum(),
                    preview.runtime().health.maximum()
                );
            }
            assert_eq!(engine.game.combat_events().iter().filter(|event| matches!(event, s::combat::CombatEvent::RespecCompleted { actor, .. } if actor.0 == 1)).count(), 1);
            assert_eq!(engine.game.combat_events().iter().filter(|event| matches!(event, s::combat::CombatEvent::Died { actor } if actor.0 == 1)).count(), usize::from(lethal));
            let after = engine.game.clone();
            let duplicate = submit(&mut engine, actor, "complete", completion.clone());
            assert!(duplicate.duplicate);
            assert_eq!(duplicate.entry, completed.entry);
            assert_eq!(engine.game, after);
            engine.flush().unwrap();
            drop(engine);
            let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
            assert_eq!(engine.game, after);
            assert!(submit(&mut engine, actor, "complete", completion).duplicate);
            assert_eq!(engine.game, after);
        }
    }
}

#[test]
fn owner_host_paused_respec_reopens_with_original_basis_and_exact_resume_identity() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("paused.db");
        let fixture = replacement_scenario(false);
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let action = replacement(&engine);
        let command = admission(&engine, action.clone());
        let accepted = submit(&mut engine, OWNER, "respec", command);
        engine.execute_next_intention().unwrap().unwrap();
        for _ in 0..10 {
            if overview(&engine).respec.unwrap().remaining_ticks < 600 {
                break;
            }
            step(&mut engine);
        }
        submit(&mut engine, OWNER, "pause", Command::PausePreparation);
        let paused = overview(&engine).respec.unwrap();
        assert!(!paused.active);
        assert!(paused.remaining_ticks > 0 && paused.remaining_ticks < paused.total_ticks);
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
        assert_eq!(overview(&engine).respec.as_ref(), Some(&paused));
        let before = engine.game.clone();
        let fresh = admission(&engine, action);
        assert!(engine
            .command(
                "owner",
                "owner-test",
                OWNER,
                "fresh",
                &engine.branch().clone(),
                fresh
            )
            .is_err());
        assert_eq!(engine.game, before);
        let resume = Command::ResumeIntention {
            expected_boundary: engine.causal_boundary(OWNER).unwrap(),
            admission: accepted.entry.id.clone(),
        };
        submit(&mut engine, OWNER, "resume", resume);
        engine.execute_next_intention().unwrap().unwrap();
        let active = overview(&engine).respec.unwrap();
        assert!(active.active);
        assert_eq!(active.basis, paused.basis);
        assert_eq!(active.intention, paused.intention);
        assert_eq!(active.replacement, paused.replacement);
        assert_eq!(active.remaining_ticks, paused.remaining_ticks);
        finish_preparation(&mut engine);
        assert!(overview(&engine).respec.is_none());
        assert_eq!(ration(&engine), 0);
        assert_eq!(
            basis(&engine).build_epoch.get(),
            paused.basis.build_epoch + 1
        );
    }
}
#[test]
fn owner_paging_constructible_maximum_ledger_and_dependent_refunds_are_fully_traversable() {
    let construction = std::time::Instant::now();
    let (fixture, source_bytes) = crate::advancement_test_support::maximum_scenario();
    // Measure the actual saved writer's package record, including escaped source
    // text. The 8 MiB raw rules-input cap does not imply metadata admission.
    let compiled =
        crate::storage::schema::scenario::encode_content_package(fixture.package.as_ref().unwrap())
            .unwrap();
    let compiled_value: serde_json::Value = serde_json::from_slice(&compiled).unwrap();
    let metadata_bytes = serde_json::to_vec(&compiled_value["package"])
        .unwrap()
        .len();
    assert!(metadata_bytes <= crate::storage::schema::scenario::MAX_SAVE_METADATA_BYTES);
    let engine = Engine::memory(fixture).unwrap();
    let construction = construction.elapsed();
    let creature = engine.game.creature(s::ActorId(1)).unwrap();
    let hd_contributions_per_fold = creature.build().choices().len();
    assert_eq!(hd_contributions_per_fold, 256);
    assert_eq!(
        creature
            .build()
            .choices()
            .iter()
            .map(|choice| choice.purchases.len())
            .sum::<usize>(),
        354
    );
    assert_eq!(creature.derived().active_talents.len(), 256);
    assert!(s::attributes::Skill::ALL.into_iter().all(|skill| creature
        .derived()
        .skills
        .get(skill)
        == 5));
    let b = engine.advancement_basis(&context(&engine)).unwrap();
    let points = overview(&engine).points;
    assert_eq!(
        (points.earned, points.spent, points.unspent),
        (65536, 354, 65182)
    );
    let before = engine.game.clone();
    let work = s::diagnostics::work_counts();
    let mut pages = 0;
    let mut max_bytes = 0;
    let mut encoded_bytes = 0usize;
    let mut metadata_queries = 0usize;
    let mut preview_queries = 0usize;
    let mut metadata_time = std::time::Duration::ZERO;
    let mut preview_time = std::time::Duration::ZERO;
    let started = std::time::Instant::now();
    let mut traverse = |make: &dyn Fn(u16) -> p::AdvancementQuery,
                        expected: usize,
                        visit: &mut dyn FnMut(&p::AdvancementEntry)| {
        let mut offset = 0;
        let mut count = 0;
        loop {
            let request = make(offset);
            let is_preview = matches!(&request, p::AdvancementQuery::Preview { .. });
            let query_work = s::diagnostics::work_counts();
            let query_start = std::time::Instant::now();
            let page = query(&engine, request);
            let elapsed = query_start.elapsed();
            let health_contributions =
                s::diagnostics::work_counts().health_derivations - query_work.health_derivations;
            if is_preview {
                preview_queries += 1;
                preview_time += elapsed;
                assert_eq!(health_contributions,2 * hd_contributions_per_fold,"each permanent/runtime fold contributes one Health calculation per HD for this condition-free surviving replacement");
            } else {
                metadata_queries += 1;
                metadata_time += elapsed;
                assert_eq!(
                    health_contributions, 0,
                    "catalog, ledger and balances do not derive candidates"
                );
            }
            assert_eq!(usize::from(page.total), expected);
            assert!(page.entries.len() <= p::MAX_ADVANCEMENT_PAGE_ENTRIES);
            let message = p::ServerMessage::AdvancementPage {
                context: context(&engine),
                request_id: "owner-test".into(),
                page: Box::new(page.clone()),
            };
            let encoded =
                p::encode_response(&message, None, p::MAX_ADVANCEMENT_PAGE_BYTES).unwrap();
            max_bytes = max_bytes.max(encoded.text.len());
            encoded_bytes += encoded.text.len();
            assert_eq!(p::decode_response(&encoded.text).unwrap(), message);
            for entry in &page.entries {
                visit(entry);
                count += 1;
            }
            pages += 1;
            let Some(next) = page.next else { break };
            assert!(next > offset);
            offset = next;
        }
        assert_eq!(count, expected);
    };
    let mut owned = std::collections::BTreeSet::new();
    traverse(
        &|offset| p::AdvancementQuery::Ledger {
            basis: b.clone(),
            offset,
        },
        354,
        &mut |entry| {
            let p::AdvancementEntry::Purchase { value } = entry else {
                panic!("ledger row")
            };
            assert!(owned.insert((value.source.owner, value.source.index)));
        },
    );
    let mut granted = 0u64;
    let mut spent = 0u64;
    traverse(
        &|offset| p::AdvancementQuery::Balances {
            basis: b.clone(),
            offset,
        },
        4096,
        &mut |entry| {
            let p::AdvancementEntry::Balance { value } = entry else {
                panic!("balance row")
            };
            granted += u64::from(value.granted);
            spent += u64::from(value.spent);
        },
    );
    assert_eq!((granted, spent), (65536, 354));
    traverse(
        &|offset| p::AdvancementQuery::Catalog {
            basis: b.clone(),
            catalog: p::AdvancementKind::Talent,
            offset,
        },
        4096,
        &mut |_| {},
    );
    let proposal = p::AdvancementProposal::Replacement {
        replacement: p::ReplacementOperation {
            source: p::ReplacementSource { owner: 0, index: 0 },
            replacement: None,
        },
    };
    let preview = |section, offset| p::AdvancementQuery::Preview {
        basis: b.clone(),
        proposal: proposal.clone(),
        section,
        offset,
    };
    let mut removed = std::collections::BTreeSet::new();
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::Removed, offset),
        256,
        &mut |entry| {
            let p::AdvancementEntry::Removed { value } = entry else {
                panic!("removed row")
            };
            assert!(removed.insert((value.source.owner, value.source.index)));
            assert!(matches!(&value.selection, p::PurchaseChoice::Talent { .. }));
        },
    );
    let mut refunds = std::collections::BTreeSet::new();
    let mut refunded = 0u64;
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::Refunds, offset),
        256,
        &mut |entry| {
            let p::AdvancementEntry::Refund { value } = entry else {
                panic!("refund row")
            };
            assert!(refunds.insert((value.owner, value.pool.id.clone())));
            refunded += u64::from(value.points);
        },
    );
    assert_eq!(refunded, 256);
    let mut candidate_spent = 0u64;
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::Balances, offset),
        4096,
        &mut |entry| {
            let p::AdvancementEntry::Balance { value } = entry else {
                panic!("candidate balance row")
            };
            candidate_spent += u64::from(value.spent);
        },
    );
    assert_eq!(candidate_spent, 98);
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::Skills, offset),
        20,
        &mut |entry| {
            let p::AdvancementEntry::Skill { value } = entry else {
                panic!("skill row")
            };
            assert_eq!(value.rank, 5);
        },
    );
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::Talents, offset),
        0,
        &mut |_| panic!("removed talents"),
    );
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::RemovedConditions, offset),
        0,
        &mut |_| panic!("no source conditions"),
    );
    traverse(
        &|offset| preview(p::AdvancementPreviewSection::Summary, offset),
        1,
        &mut |entry| {
            let p::AdvancementEntry::Summary { value } = entry else {
                panic!("summary row")
            };
            assert_eq!(value.resulting_build_epoch, b.build_epoch + 1);
        },
    );
    let after = s::diagnostics::work_counts();
    assert_eq!(
        after.game_clones, work.game_clones,
        "metadata and exact proposal queries must not clone Game"
    );
    assert_eq!(engine.game, before);
    assert_eq!(
        after.health_derivations - work.health_derivations,
        2 * hd_contributions_per_fold * preview_queries
    );
    eprintln!("joint ledger/catalog fixture with one maximum-node expression: {source_bytes} rules bytes, {metadata_bytes} saved metadata bytes, {} compiled bytes; construction {construction:?}; {pages} complete pages in {:?}; metadata {metadata_queries} queries/{metadata_time:?}; preview {preview_queries} queries/{preview_time:?}; full envelopes {encoded_bytes} bytes, maximum {max_bytes}; HD Health contributions {}",compiled.len(),started.elapsed(),after.health_derivations-work.health_derivations);
}

#[test]
fn owner_ancestry_a_b_a_build_rejects_old_basis_but_matching_receipt_still_wins() {
    let mut engine = Engine::memory(scenario()).unwrap();
    let old_basis = engine.advancement_basis(&context(&engine)).unwrap();
    let initial_build = engine.game.creature(s::ActorId(1)).unwrap().build().clone();
    let original = admission(&engine, purchase(&engine));
    let accepted = submit(&mut engine, OWNER, "initial-purchase", original.clone());
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(basis(&engine).build_epoch.get(), old_basis.build_epoch + 1);
    let refund = admission(&engine, replacement(&engine));
    submit(&mut engine, OWNER, "refund", refund);
    engine.execute_next_intention().unwrap().unwrap();
    finish_preparation(&mut engine);
    assert_eq!(
        engine.game.creature(s::ActorId(1)).unwrap().build(),
        &initial_build
    );
    assert_eq!(basis(&engine).build_epoch.get(), old_basis.build_epoch + 2);
    let before = engine.game.clone();
    let Command::AdmitIntention { action, .. } = &original else {
        unreachable!()
    };
    let stale = admission(&engine, action.clone());
    assert!(engine
        .command(
            "owner",
            "owner-test",
            OWNER,
            "new-old-basis",
            &engine.branch().clone(),
            stale
        )
        .is_err());
    assert_eq!(engine.game, before);
    assert!(engine
        .advancement_query(
            OWNER,
            engine.branch(),
            engine.causal_boundary(OWNER).unwrap(),
            &p::AdvancementQuery::Ledger {
                basis: old_basis,
                offset: 0
            },
            &PageBudget {
                context: &context(&engine),
                request_id: "stale",
                limit: p::MAX_ADVANCEMENT_PAGE_BYTES
            }
        )
        .is_err());
    let duplicate = submit(&mut engine, OWNER, "initial-purchase", original);
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.entry, accepted.entry);
    assert_eq!(engine.game, before);
    let fresh = query(
        &engine,
        p::AdvancementQuery::Ledger {
            basis: engine.advancement_basis(&context(&engine)).unwrap(),
            offset: 0,
        },
    );
    assert_eq!(fresh.total, 0);
}

#[test]
fn owner_ancestry_accepted_ancestor_purchase_survives_rewind_but_old_future_work_does_not() {
    // Wizard authority is used solely to request an existing timeline rewind.
    // Both purchases use the ordinary admission/execution path and real points.
    let mut engine = Engine::memory(scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let actor = ActorId(2);
    let mut ctx = context(&engine);
    ctx.actor = actor;
    ctx.revision = engine.causal_boundary(actor).unwrap();
    let old_branch = engine.branch().clone();
    let old_basis = engine.advancement_basis(&ctx).unwrap();
    let action = Action::Purchase {
        basis: Box::new(adapter::basis(&old_basis).unwrap()),
        purchase: s::player_advancement::PurchaseOperation {
            owner: tor_simulation::HdIndex(0),
            pool: s::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: s::player_advancement::PurchaseChoice::SkillRank {
                skill: s::attributes::Skill::Perception,
            },
        },
    };
    let command = Command::AdmitIntention {
        expected_boundary: engine.causal_boundary(actor).unwrap(),
        action: action.clone(),
    };
    let accepted = submit(&mut engine, actor, "ancestor", command.clone());
    let public_wait = Command::Act {
        expected_boundary: engine.causal_boundary(OWNER).unwrap(),
        action: Action::Wait,
    };
    let boundary = submit(&mut engine, OWNER, "public-boundary", public_wait);
    engine.execute_next_intention().unwrap().unwrap();
    ctx.revision = engine.causal_boundary(actor).unwrap();
    let future_basis = engine.advancement_basis(&ctx).unwrap();
    let future_action = Action::Purchase {
        basis: Box::new(adapter::basis(&future_basis).unwrap()),
        purchase: s::player_advancement::PurchaseOperation {
            owner: tor_simulation::HdIndex(1),
            pool: s::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: s::player_advancement::PurchaseChoice::SkillRank {
                skill: s::attributes::Skill::Athletics,
            },
        },
    };
    let future_boundary = engine.causal_boundary(actor).unwrap();
    let future = submit(
        &mut engine,
        actor,
        "future",
        Command::AdmitIntention {
            expected_boundary: future_boundary,
            action: future_action.clone(),
        },
    );
    let rewind = Command::Wizard {
        display_basis: None,
        expected_boundary: engine.causal_boundary(OWNER).unwrap(),
        operation: crate::journal::WizardOperation::Rewind {
            target: Some(boundary.entry.id),
        },
    };
    submit(&mut engine, OWNER, "rewind", rewind);
    assert_ne!(engine.branch(), &old_branch);
    assert!(engine.has_pending_intention(actor));
    let before = engine.game.clone();
    let abandoned = Command::ResumeIntention {
        expected_boundary: engine.causal_boundary(actor).unwrap(),
        admission: future.entry.id,
    };
    assert!(engine
        .command(
            "owner",
            "owner-test",
            actor,
            "resume-future",
            &engine.branch().clone(),
            abandoned
        )
        .is_err());
    assert_eq!(engine.game, before);
    let started = engine.execute_next_intention().unwrap().unwrap();
    assert!(
        matches!(&started.entry.content,JournalContent::IntentionStarted {action:stored,..} if stored==&action)
    );
    assert_eq!(
        engine
            .game
            .creature(s::ActorId(2))
            .unwrap()
            .build_epoch()
            .get(),
        future_basis.build_epoch
    );
    let before = engine.game.clone();
    let fresh_future = Command::AdmitIntention {
        expected_boundary: engine.causal_boundary(actor).unwrap(),
        action: future_action,
    };
    assert_eq!(
        engine
            .command(
                "owner",
                "owner-test",
                actor,
                "old-future-basis",
                &engine.branch().clone(),
                fresh_future
            )
            .unwrap_err()
            .code,
        p::ErrorCode::StaleContext
    );
    assert_eq!(engine.game, before);
    let duplicate = engine
        .command(
            "owner",
            "owner-test",
            actor,
            "ancestor",
            &old_branch,
            command,
        )
        .unwrap();
    assert!(duplicate.duplicate);
    assert_eq!(duplicate.entry, accepted.entry);
    assert_eq!(engine.game, before);
    let replayed = Engine::replay(engine.archive.clone(), None, None).unwrap();
    assert_eq!(replayed.game, engine.game);
}

#[test]
fn owner_ancestry_replay_rejects_forged_original_admission_actor_or_branch_basis() {
    let mut engine = Engine::memory(scenario()).unwrap();
    let command = admission(&engine, purchase(&engine));
    submit(&mut engine, OWNER, "purchase", command);
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(
        Engine::replay(engine.archive.clone(), None, None)
            .unwrap()
            .game,
        engine.game
    );
    assert_eq!(
        crate::engine::Checkpoint::capture(&engine)
            .encode("owner-ancestry", 1)
            .restore(engine.archive.clone())
            .unwrap()
            .game,
        engine.game
    );
    for foreign_actor in [false, true] {
        let mut archive = engine.archive.clone();
        for record in &mut archive.records {
            if let JournalContent::IntentionAdmitted {
                action: Action::Purchase { basis, .. },
                ..
            }
            | JournalContent::IntentionStarted {
                action: Action::Purchase { basis, .. },
                ..
            } = &mut record.entry.content
            {
                if foreign_actor {
                    basis.actor = s::ActorId(2);
                } else {
                    basis.branch = "00000000-0000-4000-8000-000000000002".into();
                }
            }
            if let Some(receipt) = &mut record.receipt {
                if let Command::AdmitIntention {
                    action: Action::Purchase { basis, .. },
                    ..
                } = &mut receipt.command
                {
                    if foreign_actor {
                        basis.actor = s::ActorId(2);
                    } else {
                        basis.branch = "00000000-0000-4000-8000-000000000002".into();
                    }
                }
            }
        }
        assert!(Engine::replay(archive.clone(), None, None).is_err());
        assert!(crate::engine::Checkpoint::capture(&engine)
            .encode("owner-ancestry", 1)
            .restore(archive)
            .is_err());
    }
}

#[test]
fn owner_form_purchase_reopens_exact_operands_receipt_and_final_grant() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("form-purchase.db");
        let fixture = crate::advancement_test_support::form_scenario(2, false);
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let selected = p::PurchaseChoice::TalentForm {
            talent: "choice_root".into(),
            form: "choice.form_0".into(),
        };
        let operation = s::player_advancement::PurchaseOperation {
            owner: tor_simulation::HdIndex(0),
            pool: s::content_ids::PointPoolId::new("advancement").unwrap(),
            selection: adapter::choice(&selected).unwrap(),
        };
        let original = engine.game.clone();
        assert!(!form_known(&engine, "spark_lance_fire"));
        assert!(!form_known(&engine, "falter"));
        let preview = engine
            .game
            .preview_player_purchase(s::ActorId(1), &basis(&engine), &operation)
            .unwrap();
        assert_eq!(
            engine.game, original,
            "preview does not publish transient grants"
        );
        assert_eq!(preview.runtime().build_epoch.get(), 2);
        assert!(
            !form_known(&engine, "spark_lance_fire"),
            "preview never licenses the form identity"
        );
        let mut stale = basis(&engine);
        stale.build_epoch = s::creatures::BuildEpoch::new(99).unwrap();
        let rejected = admission(
            &engine,
            Action::Purchase {
                basis: Box::new(stale),
                purchase: operation.clone(),
            },
        );
        assert!(engine
            .command(
                "owner",
                "owner-test",
                OWNER,
                "stale-form",
                &engine.branch().clone(),
                rejected
            )
            .is_err());
        assert_eq!(engine.game, original);
        assert!(!form_known(&engine, "spark_lance_fire"));
        let action = Action::Purchase {
            basis: Box::new(basis(&engine)),
            purchase: operation,
        };
        let command = admission(&engine, action.clone());
        let accepted = submit(&mut engine, OWNER, "form-purchase", command.clone());
        engine.flush().unwrap();
        drop(engine);
        let refusing = SavePolicy {
            max_pending_bytes: 1,
            ..policy.clone()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), refusing).unwrap();
        let unpublished = engine.game.clone();
        let saved = crate::inspect_save(&path).unwrap();
        assert_eq!(
            engine.execute_next_intention().unwrap_err().code,
            p::ErrorCode::StorageFailure
        );
        assert_eq!(
            engine.game, unpublished,
            "refused publication leaks no form grant or context change"
        );
        assert_eq!(crate::inspect_save(&path).unwrap(), saved);
        assert!(
            !form_known(&engine, "spark_lance_fire"),
            "failed Store cannot publish acquisition knowledge"
        );
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let started = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(&started.entry.content, JournalContent::IntentionStarted { action: saved, .. } if saved == &action)
        );
        assert!(
            form_known(&engine, "spark_lance_fire"),
            "finalized successful ownership licenses the acquired form"
        );
        assert!(
            !form_known(&engine, "falter"),
            "unchosen form stays private"
        );
        let creature = engine.game.creature(s::ActorId(1)).unwrap();
        assert!(creature
            .derived()
            .abilities
            .contains(&s::content_ids::AbilityId::new("spark_lance_fire").unwrap()));
        assert!(!creature
            .derived()
            .abilities
            .contains(&s::content_ids::AbilityId::new("falter").unwrap()));
        let page = query(
            &engine,
            p::AdvancementQuery::Ledger {
                basis: adapter::wire_basis(&basis(&engine)),
                offset: 0,
            },
        );
        let p::AdvancementEntry::Purchase { value } = &page.entries[0] else {
            panic!("purchase row")
        };
        assert_eq!(value.selection, selected);
        assert_eq!(
            engine.inspect_creature(OWNER).unwrap_err().code,
            p::ErrorCode::Unauthorized,
            "ordinary progression does not enable wizard inspection"
        );
        let inspection = crate::creature_view::inspect(
            &engine.game,
            s::ActorId(1),
            engine.causal_boundary(OWNER).unwrap(),
        )
        .unwrap();
        inspection.validate().unwrap();
        assert!(
            matches!(&inspection.hit_dice[0].purchases[0].selection, p::InspectionPurchaseSelection::TalentForm { talent, form } if talent.id == "choice_root" && form.id == "choice.form_0")
        );
        let completed = engine.game.clone();
        let duplicate = submit(&mut engine, OWNER, "form-purchase", command.clone());
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, accepted.entry);
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
        assert_eq!(engine.game, completed);
        assert!(form_known(&engine, "spark_lance_fire"));
        assert!(!form_known(&engine, "falter"));
        assert!(submit(&mut engine, OWNER, "form-purchase", command).duplicate);
    }
}

#[test]
fn owner_form_catalog_source_maximum_escaped_singleton_fits_complete_frame() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Engine::open(
        directory.path().join("forms.db"),
        crate::advancement_test_support::form_scenario(16, true),
    )
    .unwrap();
    let query_basis = adapter::wire_basis(&basis(&engine));
    let mut context = context(&engine);
    context.actor = p::ActorId(u64::MAX - 1);
    context.revision = u64::MAX;
    context.branch = p::BranchId("\\\"".repeat(64));
    context.input.stream.stream = p::StreamId("\\\"".repeat(64));
    context.input.stream.epoch = u64::MAX;
    context.input.readiness_revision = u64::MAX;
    context.cursor.sequence = u64::MAX;
    context.cursor.tick = u64::MAX;
    let basis = p::AdvancementBasis {
        actor: context.actor,
        branch: context.branch.clone(),
        stream: context.input.stream.stream.clone(),
        reset: u64::MAX,
        build_epoch: u64::MAX,
    };
    let mut offset = 0;
    let source = loop {
        let page = query(
            &engine,
            p::AdvancementQuery::Catalog {
                basis: query_basis.clone(),
                catalog: p::AdvancementKind::Talent,
                offset,
            },
        );
        if let Some(row) = page.entries.iter().find(|row| matches!(row, p::AdvancementEntry::Catalog { value } if value.form_choices.len() == 16)) { break row.clone(); }
        offset = page.next.expect("source root must be reachable");
    };
    let request_id = "\\\"".repeat(64);
    let budget = PageBudget {
        context: &context,
        request_id: &request_id,
        limit: p::MAX_ADVANCEMENT_PAGE_BYTES,
    };
    let query = p::AdvancementQuery::Catalog {
        basis: basis.clone(),
        catalog: p::AdvancementKind::Talent,
        offset: 4095,
    };
    let page = pack(
        basis.clone(),
        context.revision,
        query.clone(),
        4096,
        [Ok(source.clone())],
        &budget,
    )
    .unwrap();
    let message = p::ServerMessage::AdvancementPage {
        context: context.clone(),
        request_id: request_id.clone(),
        page: Box::new(page.clone()),
    };
    let encoded = p::encode_response(&message, None, budget.limit).unwrap();
    assert_eq!(p::decode_response(&encoded.text).unwrap(), message);
    assert!(encoded.text.len() < p::MAX_ADVANCEMENT_PAGE_BYTES);
    let short = PageBudget {
        limit: encoded.text.len() - 1,
        ..budget
    };
    assert_eq!(
        pack(basis, context.revision, query, 4096, [Ok(source)], &short)
            .unwrap_err()
            .code,
        p::ErrorCode::ResourceLimit
    );
    eprintln!(
        "source-validated 16-form escaped singleton full frame {} bytes",
        encoded.text.len()
    );
}

fn form_known(engine: &Engine, id: &str) -> bool {
    let mut shared = s::checkpoint::SharedState::default();
    let snapshot = engine.game.checkpoint(&mut shared);
    let value = serde_json::to_value((snapshot, shared)).unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    value[1]["actors"][actors]["1"]["recognized_abilities"]
        .as_array()
        .unwrap()
        .iter()
        .any(|known| known == id)
}

#[test]
fn owner_growth_source_preview_is_read_only_and_source_survives_retry_and_recovery() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("growth-source.db");
        let fixture = scenario();
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let initial = engine.game.clone();
        let prior_basis = basis(&engine);
        let page = query(
            &engine,
            p::AdvancementQuery::Preview {
                basis: adapter::wire_basis(&prior_basis),
                proposal: p::AdvancementProposal::GrowthSource {
                    source: p::GrowthSource::Racial {},
                },
                section: p::AdvancementPreviewSection::Summary,
                offset: 0,
            },
        );
        assert_eq!(page.total, 1);
        assert_eq!(engine.game, initial);
        let action = Action::ChooseGrowthSource {
            basis: Box::new(prior_basis.clone()),
            source: s::earned_growth::GrowthSource::Racial {},
        };
        let command = admission(&engine, action);
        let accepted = submit(&mut engine, OWNER, "choose-growth", command.clone());
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        assert_eq!(
            overview(&engine).growth.future,
            p::FutureGrowth::Selected {
                source: p::GrowthSource::Racial {}
            }
        );
        assert_eq!(
            basis(&engine).build_epoch.get(),
            prior_basis.build_epoch.get() + 1
        );
        let completed = engine.game.clone();
        let duplicate = submit(&mut engine, OWNER, "choose-growth", command.clone());
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, accepted.entry);
        assert_eq!(engine.game, completed);
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture, policy).unwrap();
        assert_eq!(engine.game, completed);
        assert!(submit(&mut engine, OWNER, "choose-growth", command).duplicate);
    }
}

#[test]
fn growth_source_rewind_restores_initial_policy_and_invalidates_abandoned_basis() {
    let mut engine = Engine::memory(scenario()).unwrap();
    engine.enable_wizard().unwrap();
    let initial = overview(&engine).growth;
    let action = Action::ChooseGrowthSource {
        basis: Box::new(basis(&engine)),
        source: s::earned_growth::GrowthSource::Racial {},
    };
    let command = admission(&engine, action);
    submit(&mut engine, OWNER, "choose-source", command);
    engine.execute_next_intention().unwrap().unwrap();
    let abandoned = basis(&engine);
    assert_ne!(overview(&engine).growth.future, initial.future);
    let rewind = Command::Wizard {
        display_basis: None,
        expected_boundary: engine.causal_boundary(OWNER).unwrap(),
        operation: crate::journal::WizardOperation::Rewind { target: None },
    };
    submit(&mut engine, OWNER, "rewind-growth", rewind);
    assert_eq!(overview(&engine).growth, initial);
    let unchanged = engine.game.clone();
    let action = Action::ChooseGrowthSource {
        basis: Box::new(abandoned),
        source: s::earned_growth::GrowthSource::Class {
            class: s::content_ids::ClassId::new("warrior").unwrap(),
        },
    };
    let command = admission(&engine, action);
    assert!(engine
        .command(
            "owner",
            "owner-test",
            OWNER,
            "abandoned-source",
            &engine.branch().clone(),
            command
        )
        .is_err());
    assert_eq!(engine.game, unchanged);
}

#[test]
fn earned_fatal_growth_survives_receipts_checkpoint_replay_and_rewind_branches() {
    use serde_json::json;
    let base = scenario();
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["factions"] = json!({"allies":["opponents"],"opponents":["allies"]});
    manifest["characters"][0]["creature"]["hit_dice"] = json!([{"source":"racial"}]);
    manifest["characters"][0]["creature"]["future_growth"] =
        json!({"kind":"selected","source":{"kind":"racial"}});
    manifest["creatures"]["species"]["learner"]["melee"]["strikes"][0]["on_success"][0]["damage"]
        ["components"][0]["amount"]["amount"] = json!(100);
    let mut regions = package.region_defs().unwrap();
    let mut first = serde_json::to_value(&regions[0]).unwrap();
    first["actors"][0]["at"] = json!([2, 1, 0]);
    first["actors"][0]["creature"]["faction"] = json!("opponents");
    regions[0] = serde_json::from_value(first).unwrap();
    let mut rules: s::rules::RulesetDocument =
        toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
    rules.growth.reward_base = 100;
    rules.growth.reward_per_relative_tier = 0;
    rules.growth.minimum_reward = 100;
    rules.growth.maximum_reward = 100;
    let fixture = Scenario {
        package: Some(std::sync::Arc::new(
            crate::scenario_package::Package::from_parts(
                serde_json::from_value(manifest).unwrap(),
                regions,
                &toml::to_string(&rules).unwrap(),
            )
            .unwrap(),
        )),
        ..base
    };
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("earned-fatal.db");
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let command = admission(
            &engine,
            Action::UseAbility {
                id: s::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::journal::AbilityInvocation {
                    aim: crate::journal::AbilityAim::ActorTarget {
                        target: s::ActorId(2),
                    },
                    weapon: crate::journal::WeaponSelection::Default,
                },
                supplies: SupplySelection::Carried,
            },
        );
        let accepted = submit(&mut engine, OWNER, "fatal-admission", command.clone());
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        engine.execute_next_intention().unwrap().unwrap();
        step(&mut engine);
        assert!(!engine.game.alive(s::ActorId(2)));
        assert_eq!(overview(&engine).growth.earned_xp, 100);
        assert_eq!(overview(&engine).growth.spent_xp, 100);
        assert_eq!(
            engine
                .game
                .creature(s::ActorId(1))
                .unwrap()
                .build()
                .ledger()
                .total_hd(),
            tor_simulation::HdCount(2)
        );
        let completed = engine.game.clone();
        let duplicate = submit(&mut engine, OWNER, "fatal-admission", command.clone());
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, accepted.entry);
        assert_eq!(engine.game, completed);
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            completed
        );
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy).unwrap();
        assert_eq!(engine.game, completed);
        assert!(submit(&mut engine, OWNER, "fatal-admission", command).duplicate);
        engine.enable_wizard().unwrap();
        let branch = engine.branch().clone();
        let rewind = Command::Wizard {
            display_basis: None,
            expected_boundary: engine.causal_boundary(OWNER).unwrap(),
            operation: crate::journal::WizardOperation::Rewind { target: None },
        };
        submit(&mut engine, OWNER, "rewind-before-death", rewind);
        assert_ne!(engine.branch(), &branch);
        assert!(engine.game.alive(s::ActorId(2)));
        assert_eq!(overview(&engine).growth.earned_xp, 0);
        let command = admission(
            &engine,
            Action::UseAbility {
                id: s::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::journal::AbilityInvocation {
                    aim: crate::journal::AbilityAim::ActorTarget {
                        target: s::ActorId(2),
                    },
                    weapon: crate::journal::WeaponSelection::Default,
                },
                supplies: SupplySelection::Carried,
            },
        );
        submit(&mut engine, OWNER, "new-branch-fatal", command);
        engine.execute_next_intention().unwrap().unwrap();
        step(&mut engine);
        assert!(!engine.game.alive(s::ActorId(2)));
        assert_eq!(overview(&engine).growth.earned_xp, 100);
        assert_eq!(overview(&engine).growth.spent_xp, 100);
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            engine.game
        );
    }
}

#[test]
fn owner_expertise_catalog_preview_paid_commit_and_reopen_preserve_exact_source() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("expertise.db");
        let fixture = crate::advancement_test_support::expertise_scenario();
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), policy.clone()).unwrap();
        let before = engine.game.clone();
        let selection = p::PurchaseChoice::Expertise {
            expertise: "fixture.knife".into(),
        };
        let wire_basis = adapter::wire_basis(&basis(&engine));
        let catalog = query(
            &engine,
            p::AdvancementQuery::Catalog {
                basis: wire_basis.clone(),
                catalog: p::AdvancementKind::Expertise,
                offset: 0,
            },
        );
        assert_eq!(catalog.total, 1);
        assert!(
            matches!(&catalog.entries[0], p::AdvancementEntry::Catalog { value } if value.selection == selection && value.name == "Knife")
        );
        let purchase = p::PurchaseOperation {
            owner: 0,
            pool: "advancement".into(),
            selection,
        };
        let preview = query(
            &engine,
            p::AdvancementQuery::Preview {
                basis: wire_basis,
                proposal: p::AdvancementProposal::Purchase {
                    purchase: purchase.clone(),
                },
                section: p::AdvancementPreviewSection::Expertises,
                offset: 0,
            },
        );
        assert_eq!(preview.total, 1);
        let p::AdvancementEntry::Expertise { value } = &preview.entries[0] else {
            panic!("expertise preview")
        };
        assert_eq!(value.status, p::AdvancementStatus::Active);
        assert_eq!(value.source_count, 1);
        assert_eq!(engine.game, before, "preview is descriptive only");
        let action = Action::Purchase {
            basis: Box::new(basis(&engine)),
            purchase: adapter::purchase(&purchase).unwrap(),
        };
        let command = admission(&engine, action.clone());
        submit(&mut engine, OWNER, "expertise", command.clone());
        engine.flush().unwrap();
        drop(engine);
        let mut engine =
            Engine::open_with_policy(&path, Scenario::two_room(999), policy.clone()).unwrap();
        let started = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(&started.entry.content, JournalContent::IntentionStarted { action: saved, .. } if saved == &action)
        );
        let id = s::content_ids::ExpertiseId::new("fixture.knife").unwrap();
        let creature = engine.game.creature(s::ActorId(1)).unwrap();
        let derived = creature.derived();
        assert!(derived.expertises.active(&id));
        let rows = crate::creature_view::expertise_rows(
            &derived.expertises,
            engine
                .game
                .creature(s::ActorId(1))
                .unwrap()
                .build()
                .ruleset(),
        );
        assert_eq!(&rows[0], value);
        let completed = engine.game.clone();
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, Scenario::two_room(999), policy).unwrap();
        assert_eq!(engine.game, completed);
        assert!(submit(&mut engine, OWNER, "expertise", command).duplicate);
        assert_eq!(engine.game, completed);
    }
}

#[test]
fn intellect_entitlement_preview_commit_stale_basis_and_source_free_reopen() {
    for checkpoint_interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("intellect-expertise.db");
        let policy = SavePolicy {
            checkpoint_interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(
            &path,
            crate::advancement_test_support::expertise_scenario_with_intellect(1),
            policy.clone(),
        )
        .unwrap();
        let before = engine.game.clone();
        let points = engine
            .game
            .creature(s::ActorId(1))
            .unwrap()
            .build()
            .unspent_points();
        let old_basis = basis(&engine);
        let choice = p::IntellectExpertiseOperation {
            threshold: 1,
            expertise: "fixture.knife".into(),
        };
        let rows = query(
            &engine,
            p::AdvancementQuery::IntellectExpertises {
                basis: adapter::wire_basis(&old_basis),
                offset: 0,
            },
        );
        assert_eq!(rows.total, 1);
        assert!(
            matches!(&rows.entries[0], p::AdvancementEntry::IntellectExpertise { value } if value.threshold == 1 && value.expertise.is_none() && value.status == p::AdvancementStatus::Active)
        );
        let preview = query(
            &engine,
            p::AdvancementQuery::Preview {
                basis: adapter::wire_basis(&old_basis),
                proposal: p::AdvancementProposal::IntellectExpertise {
                    choice: choice.clone(),
                },
                section: p::AdvancementPreviewSection::Expertises,
                offset: 0,
            },
        );
        assert_eq!(preview.total, 1);
        assert_eq!(engine.game, before);
        let action = Action::ChooseIntellectExpertise {
            basis: Box::new(old_basis.clone()),
            choice: adapter::intellect_expertise(&choice).unwrap(),
        };
        let command = admission(&engine, action.clone());
        let accepted = submit(&mut engine, OWNER, "intellect-choice", command.clone());
        engine.flush().unwrap();
        drop(engine);
        let mut engine =
            Engine::open_with_policy(&path, Scenario::two_room(999), policy.clone()).unwrap();
        let started = engine.execute_next_intention().unwrap().unwrap();
        assert!(
            matches!(&started.entry.content, JournalContent::IntentionStarted { action: saved, .. } if saved == &action)
        );
        let creature = engine.game.creature(s::ActorId(1)).unwrap();
        let id = s::content_ids::ExpertiseId::new("fixture.knife").unwrap();
        assert!(creature.derived().expertises.active(&id));
        assert_eq!(creature.build().unspent_points(), points);
        assert!(matches!(
            engine.game.preview_player_intellect_expertise(
                s::ActorId(1),
                &old_basis,
                &adapter::intellect_expertise(&choice).unwrap()
            ),
            Err(s::GameError::Build(
                s::creatures::BuildError::StaleAdvancementPreview
            ))
        ));
        let completed = engine.game.clone();
        let duplicate = submit(&mut engine, OWNER, "intellect-choice", command.clone());
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, accepted.entry);
        assert_eq!(engine.game, completed);
        assert_eq!(
            Engine::replay(engine.archive.clone(), None, None)
                .unwrap()
                .game,
            completed
        );
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&path, Scenario::two_room(999), policy).unwrap();
        assert_eq!(engine.game, completed);
        assert!(submit(&mut engine, OWNER, "intellect-choice", command).duplicate);
        let rows = query(
            &engine,
            p::AdvancementQuery::IntellectExpertises {
                basis: adapter::wire_basis(&basis(&engine)),
                offset: 0,
            },
        );
        assert!(
            matches!(&rows.entries[0], p::AdvancementEntry::IntellectExpertise { value } if value.expertise.as_ref().is_some_and(|value| value.id == "fixture.knife"))
        );
    }
}

#[test]
fn owner_overview_scales_current_action_timings_but_not_received_loss_clock() {
    for (rate, purchase, prepare, recovery) in [
        ("normal", 3, 601, 5),
        ("haste", 2, 301, 3),
        ("slow", 6, 1202, 10),
    ] {
        let base = scenario();
        let package = base.package.as_ref().unwrap();
        let mut manifest = serde_json::to_value(&package.manifest).unwrap();
        manifest["creatures"]["species"]["learner"]["modifiers"] = serde_json::json!([{
            "scope":"always", "op":{"kind":"action_duration", "value":rate}
        }]);
        let mut rules: s::rules::RulesetDocument =
            toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
        rules.advancement.purchase_recovery = 3;
        rules.advancement.respec_prepare = 601;
        rules.advancement.respec_recovery = 5;
        rules.advancement.received_loss_interval = 701;
        let package = crate::scenario_package::Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap();
        let engine = Engine::memory(Scenario {
            package: Some(std::sync::Arc::new(package)),
            ..base
        })
        .unwrap();
        let before = engine.game.clone();
        let view = overview(&engine);
        assert_eq!(view.timings.purchase_recovery, purchase, "{rate}");
        assert_eq!(view.timings.respec_prepare, prepare, "{rate}");
        assert_eq!(view.timings.respec_recovery, recovery, "{rate}");
        assert_eq!(view.timings.received_loss_interval, 701, "{rate}");
        assert!(view.respec.is_none());
        assert_eq!(engine.game, before, "overview remains read-only");
    }
}

#[test]
fn actionable_balance_rows_exclude_locked_package_pools() {
    use s::content_ids::BodyFormId;
    use s::creatures::{BodyPackage, PlannedAdvancement};
    use std::sync::Arc;
    let engine = Engine::memory(scenario()).unwrap();
    let creature = engine.game.creature(s::ActorId(1)).unwrap();
    let source = creature.build();
    let ordinary = CreatureBuild::new(
        source.content().clone(),
        source.species_id().clone(),
        source.ledger().clone(),
        source.binding(),
        *source.starting_skills(),
    )
    .unwrap();
    let root = ordinary.ledger().health_root_seed().unwrap();
    let base = Arc::new(BodyPackage {
        id: BodyFormId::new("balance_body").unwrap(),
        species: ordinary.species_id().clone(),
        body: s::BodySpec::default(),
        advancement: vec![PlannedAdvancement {
            source: ordinary.ledger().entries()[0].source(),
            choices: ordinary.choices()[0].clone(),
        }],
    });
    assert_eq!(
        balances(&ordinary).unwrap().len(),
        2,
        "ordinary ownership is unchanged"
    );
    let owned = ordinary.with_body_package(base, root).unwrap();
    let rows = balances(&owned).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, s::HdIndex(1));
    assert_eq!(rows[0].1.unspent(), 1);
    assert_eq!(
        owned.point_balances(s::HdIndex(0)).unwrap()[0].unspent(),
        1,
        "package grant is retained, not transferred or destroyed"
    );
}
