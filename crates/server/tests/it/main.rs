//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod background_save;
mod checkpoints;
mod command_boundary;
mod descriptions;
mod doors;
mod dungeon;
mod generated_regions;
mod geometry;
mod history;
mod invariants;
mod items;
mod materials;
mod package_pin;
mod palettes;
mod performance_contracts;
mod performance_workloads;
mod physics;
mod place_hints;
mod process;
mod recovery_fixtures;
mod region_horizon;
mod region_preloading;
mod region_streaming;
mod scenario_packages;
mod streaming_websocket;
mod support;
mod travel;
mod websocket;
mod wizard;
mod wizard_websocket;
