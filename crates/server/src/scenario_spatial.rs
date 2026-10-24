//! Canonical spatial compilation boundary. Internal trusted source inputs only;
//! no public authored schema, saved registry or gameplay integration yet.
use super::Package;
use crate::Failure;
use std::collections::BTreeMap;
use tor_protocol::ErrorCode;
use tor_simulation::spatial_scope::{
    registry::{Budget, Scope},
    RoomId,
};
use tor_world::{Location, RegionId, World};

#[derive(Clone, Debug)]
pub(super) struct CanonicalSpatialSource {
    pub id: String,
    pub members: Vec<RegionId>,
    pub rooms: BTreeMap<RoomId, Vec<Location>>,
    pub passages: Vec<Location>,
    /// Room-primary locations; passage affiliation is derived, never supplied.
    pub doorways: Vec<Location>,
}

pub(super) fn compile(
    world: &World,
    source: &CanonicalSpatialSource,
    budget: &Budget,
) -> Result<Scope, Failure> {
    // Honest first-red boundary. Green must bound input work before expansion,
    // derive components over passage-primary PLUS doorway cells (not ordinary
    // room interiors or abstract stairs), and certify complete ownership.
    let _ = (
        world,
        &source.id,
        &source.members,
        &source.rooms,
        &source.passages,
        &source.doorways,
        budget,
    );
    Err(Failure::new(
        ErrorCode::InvalidAction,
        "Canonical spatial compiler is not implemented",
    ))
}

pub(super) fn generated_source(
    package: &Package,
    identity: &str,
    seed: u64,
    budget: &Budget,
) -> Result<CanonicalSpatialSource, Failure> {
    // Must eventually use final recipe labels, including maze F_PASS, not
    // population's floor-only PreparedRoomGroup as a room-volume substitute.
    let _ = (package, identity, seed, budget);
    Err(Failure::new(
        ErrorCode::InvalidAction,
        "Generated spatial source is not implemented",
    ))
}

#[path = "scenario_spatial_tests.rs"]
mod tests;
