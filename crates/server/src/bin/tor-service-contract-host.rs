//! Opt-in stdio adapter for production Service contracts, not a server frontend.
fn main() -> std::io::Result<()> {
    tor_server::run_service_contract_host(std::io::stdin().lock(), std::io::stdout().lock())
}
