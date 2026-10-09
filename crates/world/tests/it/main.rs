//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod diagonal;
mod geometry_snapshot;
mod materials;
mod passages;
mod place_hints;
mod region_bounds;
mod rotations;
mod scene;
mod shadowcasting;
mod sight3d;
mod sight_cache;
mod visibility;

mod lighting;
