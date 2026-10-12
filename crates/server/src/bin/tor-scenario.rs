use std::io::Read;
use std::path::Path;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, source, destination] if command == "export" => {
            tor_server::scenario_package::export(Path::new(source), Path::new(destination))?;
            println!("{}", serde_json::json!({"exported": destination}));
            Ok(())
        }
        [command, directory] if command == "check" => {
            let certificate = tor_server::scenario_package::check(Path::new(directory))?;
            println!("{}", serde_json::to_string_pretty(&certificate)?);
            Ok(())
        }
        [command, directory] if command == "inspect" => {
            let resolved = tor_server::scenario_package::resolve_source(Path::new(directory))?;
            println!("{}", serde_json::to_string_pretty(&resolved)?);
            Ok(())
        }
        [command] if command == "resolve-rules" => {
            let mut source = String::new();
            std::io::stdin().take(8 * 1024 * 1024 + 1).read_to_string(&mut source)?;
            let resolved = tor_server::scenario_package::resolve_rules_source(&source, "stdin")?;
            println!("{}", serde_json::to_string_pretty(&resolved)?);
            Ok(())
        }
        [command, directory] if command == "validate" => {
            let certificate = tor_server::scenario_package::validate(Path::new(directory))?;
            println!("{}", serde_json::to_string_pretty(&certificate)?);
            Ok(())
        }
        [command, directory, region, hops] if command == "horizon" => {
            let scenario = tor_server::scenario_package::load(Path::new(directory), 0, None, false)?;
            let catalog = tor_server::region_streaming::RegionCatalog::from_package(
                scenario.package.as_ref().ok_or("Missing authored package")?,
            )?;
            let roots = std::collections::BTreeSet::from([tor_world::RegionId(region.parse()?)]);
            let plan = catalog.plan(&roots, hops.parse()?, &Default::default())?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
            Ok(())
        }
        _ => Err("Usage: tor-scenario check|inspect|validate <package-directory> | export <package-directory> <new-directory> | resolve-rules < rules.toml | horizon <package-directory> <region-id> <portal-hops>".into()),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"error": {"code": "scenario_invalid", "operation": std::env::args().nth(1), "message": error.to_string()}})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
