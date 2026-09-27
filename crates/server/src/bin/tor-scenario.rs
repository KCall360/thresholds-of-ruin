use std::path::Path;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, directory] if command == "validate" => {
            let certificate = tor_server::scenario_package::validate(Path::new(directory))?;
            println!("{}", serde_json::to_string_pretty(&certificate)?);
            Ok(())
        }
        _ => Err("Usage: tor-scenario validate <package-directory>".into()),
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
