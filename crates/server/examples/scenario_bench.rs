//! Package workload v1: integrity checks, deterministic construction and action cost.
use std::time::Instant;
use tor_server::{scenario_package, Engine, Scenario};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let original = scenario_package::load(&root, 42, None, false)?;
    for count in [2, 256] {
        let directory = tempfile::tempdir()?;
        let source = original.package.as_ref().unwrap();
        let mut regions = source.region_defs()?;
        for id in 3..=count {
            let mut region = regions[1].clone();
            region.id = id;
            region.portals.clear();
            region.items.clear();
            region.doors.clear();
            regions.push(region);
        }
        scenario_package::write_package(directory.path(), &source.manifest, &regions)?;
        let start = Instant::now();
        scenario_package::validate(directory.path())?;
        let validation_ms = start.elapsed().as_secs_f64() * 1000.0;
        let mut bytes = std::fs::metadata(directory.path().join("scenario.toml"))?.len();
        for entry in std::fs::read_dir(directory.path().join("regions"))? {
            bytes += entry?.metadata()?.len();
        }
        for sample in 0..20 {
            let start = Instant::now();
            let scenario = scenario_package::load(directory.path(), 42, None, false)?;
            let integrity_ms = start.elapsed().as_secs_f64() * 1000.0;
            let start = Instant::now();
            let mut engine = Engine::memory(scenario.clone())?;
            let construction_ms = start.elapsed().as_secs_f64() * 1000.0;
            let actor = tor_protocol::ActorId(1);
            let start = Instant::now();
            engine.command(
                "benchmark",
                "scenario",
                actor,
                "wait",
                &engine.branch().clone(),
                tor_server::journal::Command::Act {
                    expected_revision: 0,
                    action: tor_protocol::Action::Wait,
                },
            )?;
            let action_ms = start.elapsed().as_secs_f64() * 1000.0;
            let legacy_construction_ms = if count == 2 {
                let start = Instant::now();
                let _legacy = Engine::memory(Scenario::two_room(42))?;
                Some(start.elapsed().as_secs_f64() * 1000.0)
            } else {
                None
            };
            let save = directory.path().join(format!("sample-{sample}.db"));
            let policy = tor_server::SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            };
            let start = Instant::now();
            let mut durable = Engine::open_with_policy(&save, scenario, policy.clone())?;
            let create_save_ms = start.elapsed().as_secs_f64() * 1000.0;
            durable.command(
                "benchmark",
                "scenario",
                actor,
                "wait",
                &durable.branch().clone(),
                tor_server::journal::Command::Act {
                    expected_revision: 0,
                    action: tor_protocol::Action::Wait,
                },
            )?;
            let expected = durable.state(actor)?;
            let start = Instant::now();
            durable.flush()?;
            let save_barrier_ms = start.elapsed().as_secs_f64() * 1000.0;
            drop(durable);
            let saved_bytes = std::fs::metadata(&save)?.len();
            let start = Instant::now();
            let resumed = Engine::open_with_policy(&save, Scenario::two_room(999), policy)?;
            let resume_ms = start.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(resumed.state(actor)?, expected);
            assert_eq!(resumed.scenario_validation(), Some(true));
            println!(
                "{}",
                serde_json::json!({"workload_version":1,"regions":count,"sample":sample,"source_bytes":bytes,"saved_bytes":saved_bytes,"create_save_ms":create_save_ms,"save_barrier_ms":save_barrier_ms,"resume_ms":resume_ms,"validation_ms":validation_ms,"integrity_ms":integrity_ms,"construction_ms":construction_ms,"action_ms":action_ms,"legacy_construction_ms":legacy_construction_ms})
            );
        }
    }
    Ok(())
}
