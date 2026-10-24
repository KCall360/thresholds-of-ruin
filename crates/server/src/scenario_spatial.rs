//! Canonical spatial compilation boundary. Internal trusted source inputs only;
//! no public authored schema, saved registry or gameplay integration yet.
use super::Package;
use crate::Failure;
use std::collections::{BTreeMap, BTreeSet};
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
    /// Explicit ordinary room ownership, separate from true interior volume.
    pub doorways: BTreeMap<Location, RoomId>,
    /// Exact retained closure face cells, never an extra six-face dilation basis.
    pub closure_faces: BTreeSet<Location>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CompilerWorkBudget {
    pub source_entries: u64,
    pub terrain_cells: u64,
}

pub(super) fn compile(
    world: &World,
    source: &CanonicalSpatialSource,
    budget: &Budget,
    work: &CompilerWorkBudget,
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
        &source.closure_faces,
        work.source_entries,
        work.terrain_cells,
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
    work: &CompilerWorkBudget,
) -> Result<CanonicalSpatialSource, Failure> {
    // Must eventually use final recipe labels, including maze F_PASS, not
    // population's floor-only PreparedRoomGroup as a room-volume substitute.
    let _ = (
        package,
        identity,
        seed,
        budget,
        work.source_entries,
        work.terrain_cells,
    );
    Err(Failure::new(
        ErrorCode::InvalidAction,
        "Generated spatial source is not implemented",
    ))
}

#[path = "scenario_spatial_tests.rs"]
mod tests;
