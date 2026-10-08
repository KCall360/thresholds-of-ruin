//! Items workload v1. Timed phases exclude JSON reporting and authored setup.
use std::{path::Path, time::Instant};
use tor_protocol::ActorId;
use tor_server::journal::Action;
use tor_server::journal::Command;
use tor_server::{scenario_package, Engine, SavePolicy, Scenario};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
    let original = scenario_package::load(&root, 42, None, false)?;
    for (items, identities) in [(16_u64, 8_u64), (1000, 256)] {
        let directory = tempfile::tempdir()?;
        let mut package = (**original.package.as_ref().unwrap()).clone();
        package.manifest.characters.truncate(1);
        let mut regions = package.region_defs()?;
        let template = regions[0].items[0].clone();
        regions[0].items.clear();
        let archetype = package.manifest.archetypes["healing"].clone();
        for id in 0..identities {
            let key = format!("identity-{id}");
            let mut a = archetype.clone();
            a.identity = Some(key.clone());
            a.name = Some(format!("effect-{id}"));
            package.manifest.archetypes.insert(key.clone(), a);
            if id + 1 < identities {
                package.manifest.characters[0].known_identities.push(key);
            }
        }
        for id in 1..=items {
            let mut item = template.clone();
            item.id = id;
            item.quantity = 100;
            item.archetype = Some(format!("identity-{}", id % identities));
            regions[0].items.push(item);
        }
        scenario_package::write_package(directory.path(), &package.manifest, &regions)?;
        scenario_package::validate(directory.path())?;
        for sample in 0..20 {
            let scenario = scenario_package::load(directory.path(), 42, None, false)?;
            let start = Instant::now();
            let mut engine = Engine::memory(scenario.clone())?;
            let construction_ms = start.elapsed().as_secs_f64() * 1000.;
            let actor = ActorId(1);
            engine.enable_wizard()?;
            let branch = engine.branch().clone();
            let start = Instant::now();
            engine.command(
                "benchmark",
                "items-v1",
                actor,
                "identify",
                &branch,
                Command::Wizard {
                    expected_revision: 0,
                    operation: tor_server::journal::WizardOperation::IdentifyItem {
                        actor,
                        item: identities - 1,
                    },
                },
            )?;
            let knowledge_ms = start.elapsed().as_secs_f64() * 1000.;
            let mut app = tor_client_ascii::App::new();
            app.role = tor_protocol::AccessRole::Player;
            app.set_state(
                tor_client_common::ClientState::from_snapshot(tor_protocol::Snapshot {
                    readiness: tor_protocol::Readiness {
                        revision: 0,
                        admission: false,
                        resume: vec![],
                        cancel: vec![],
                    },
                    context: fixture_context(),
                    intentions: Vec::new(),
                    travel: None,
                    actor,
                    branch: engine.branch().clone(),
                    cursor: tor_protocol::StreamCursor {
                        sequence: 0,
                        tick: 0,
                    },
                    state: engine.state(actor)?.into(),
                    has_control: true,
                    history: tor_protocol::HistoryPage {
                        entries: vec![],
                        older_before: None,
                    },
                })
                .map_err(|e| format!("Client snapshot: {e:?}"))?,
            );
            app.ready();
            let mut canvas = tor_client_ascii::render::Canvas::default();
            let before = tor_simulation::diagnostics::work_counts();
            let mut transfer_ms = Vec::new();
            let mut client_apply_ms = Vec::new();
            let mut client_render_ms = Vec::new();
            for turn in 0..20 {
                let action = if turn % 2 == 0 {
                    Action::Take {
                        item: 1,
                        quantity: Some(1),
                    }
                } else {
                    engine.decode_action(
                        actor,
                        &tor_protocol::Action::Drop {
                            item: engine.state(actor)?.observation.inventory[0].id,
                            quantity: None,
                        },
                    )?
                };
                let revision = engine.revision(actor).unwrap();
                let branch = engine.branch().clone();
                let start = Instant::now();
                let result = engine.command(
                    "benchmark",
                    "items-v1",
                    actor,
                    &format!("{turn}"),
                    &branch,
                    Command::Act {
                        expected_revision: revision,
                        action,
                    },
                )?;
                transfer_ms.push(start.elapsed().as_secs_f64() * 1000.);
                let state = engine.state(actor)?;
                let update = tor_protocol::StreamUpdate {
                    context: fixture_context(),
                    actor,
                    branch: branch.clone(),
                    cursor: tor_protocol::StreamCursor {
                        sequence: turn + 1,
                        tick: state.observation.tick,
                    },
                    body: tor_protocol::UpdateBody::Observation {
                        state: state.into(),
                        event: Some(Box::new(
                            engine
                                .disclose_entry(&result.entry)
                                .expect("completed command has history"),
                        )),
                    },
                };
                let start = Instant::now();
                app.update(update)
                    .map_err(|e| format!("Client update: {e:?}"))?;
                client_apply_ms.push(start.elapsed().as_secs_f64() * 1000.);
                let start = Instant::now();
                canvas.draw(std::hint::black_box(&app));
                std::hint::black_box(&canvas.pixels);
                client_render_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
            let after = tor_simulation::diagnostics::work_counts();
            let state = engine.state(actor)?;
            let disclosed_bytes = serde_json::to_vec(&state)?.len();
            let path = directory.path().join(format!("{sample}.db"));
            let policy = SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            };
            let mut durable = Engine::open_with_policy(&path, scenario, policy.clone())?;
            durable.command(
                "benchmark",
                "items-v1",
                actor,
                "take",
                &durable.branch().clone(),
                Command::Act {
                    expected_revision: 0,
                    action: Action::Take {
                        item: 1,
                        quantity: Some(1),
                    },
                },
            )?;
            let expected = durable.state(actor)?;
            let start = Instant::now();
            durable.flush()?;
            let save_ms = start.elapsed().as_secs_f64() * 1000.;
            drop(durable);
            let saved_bytes = std::fs::metadata(&path)?.len();
            let start = Instant::now();
            let resumed = Engine::open_with_policy(&path, Scenario::two_room(0), policy)?;
            let resume_ms = start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(resumed.state(actor)?, expected);
            println!(
                "{}",
                serde_json::json!({"workload_version":1,"items":items,"identities":identities,
                "sample":sample,"transfers":20,"construction_ms":construction_ms,"transfer_ms":transfer_ms,
                "knowledge_ms":knowledge_ms,
                "client_apply_ms":client_apply_ms,"client_render_ms":client_render_ms,
                "observations":after.observations-before.observations,"scenes":after.scenes-before.scenes,
                "item_candidates":after.item_candidates-before.item_candidates,"stack_candidates":after.stack_candidates-before.stack_candidates,
                "knowledge_checks":after.knowledge_checks-before.knowledge_checks,
                "disclosed_bytes":disclosed_bytes,"saved_bytes":saved_bytes,"save_ms":save_ms,"resume_ms":resume_ms})
            );
        }
    }
    Ok(())
}

/// Context for one synthetic attachment used by this fixture/workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}
