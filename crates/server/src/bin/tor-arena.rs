//! Fresh offline arena runs; no changes to authored packages or existing saves.
use std::{path::PathBuf, sync::Arc};
use tor_server::{
    arena_evaluation,
    scenario_package::{self, ArenaControl, Package},
};

fn evaluate() -> Result<arena_evaluation::ArenaEvaluation, String> {
    let mut path = None;
    let mut seed = None;
    let mut all_ai = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--scenario" if path.is_none() => {
                path = Some(PathBuf::from(args.next().ok_or("Missing scenario path")?))
            }
            "--seed" if seed.is_none() => {
                seed = Some(
                    args.next()
                        .ok_or("Missing seed")?
                        .parse::<u64>()
                        .map_err(|_| "Invalid seed")?,
                )
            }
            "--all-ai" if !all_ai => all_ai = true,
            "--help" | "-h" => {
                println!("tor-arena --scenario PATH [--seed N] [--all-ai]");
                std::process::exit(0);
            }
            _ => return Err("Unknown or duplicate argument".into()),
        }
    }
    let path = path.ok_or("Supply --scenario PATH")?;
    let mut scenario = scenario_package::load(&path, seed.unwrap_or(42), None, false)
        .map_err(|e| e.to_string())?;
    if all_ai {
        let package = scenario.package.take().ok_or("Missing scenario package")?;
        let mut manifest = package.manifest.clone();
        let arena = manifest.arena.as_mut().ok_or("Scenario is not an arena")?;
        arena.control = ArenaControl::AllAi;
        arena.start_paused = false;
        let mut resolved =
            Package::from_parts(manifest, package.region_defs().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        resolved.selected = package.selected;
        scenario.package = Some(Arc::new(resolved));
    }
    arena_evaluation::run(scenario).map_err(|e| e.to_string())
}

fn main() {
    match evaluate() {
        Ok(report) => {
            if let Err(error) = serde_json::to_writer(std::io::stdout().lock(), &report) {
                eprintln!("Arena report: {error}");
                std::process::exit(2);
            }
            println!();
        }
        Err(message) => {
            println!(
                "{}",
                serde_json::json!({"format": "tor-arena-run-v1", "status": "failure", "message": message})
            );
            std::process::exit(2);
        }
    }
}
