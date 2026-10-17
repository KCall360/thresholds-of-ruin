//! Native entry point; application and behavior are independently testable.
fn main() -> Result<(), tor_client_3d::Error> {
    tor_client_3d::run()
}
