use std::path::Path;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
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
        _ => Err("Usage: tor-scenario validate <package-directory> | horizon <package-directory> <region-id> <portal-hops>".into()),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"error": {"code": "scenario_invalid", "message": error.to_string()}})
            );
            std::process::ExitCode::FAILURE
        }
    }
}
