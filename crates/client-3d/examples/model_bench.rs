fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().len() != 1 {
        return Err("model_bench accepts no arguments; the bounded workload is versioned".into());
    }
    // This standalone process owns its environment before starting any workers.
    // Diagnostic cache-miss logging must not become part of timed construction.
    std::env::remove_var("TOR_3D_PROFILE_BUILD");
    tor_client_3d::model_benchmark::run()
}
