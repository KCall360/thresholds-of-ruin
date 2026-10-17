//! GPU uniform layout. ShaderType emits unused compile-time size checks.
#![allow(dead_code)]
use bevy::{prelude::*, render::render_resource::ShaderType};

/// Shader inputs: the cutaway and the motif's palette.
#[derive(ShaderType, Reflect, Debug, Clone)]
pub struct CaveParams {
    /// xyz: the player's chest; w: the floor surface under the player.
    pub player: Vec4,
    /// xyz: the camera; w: cutaway radius.
    pub camera: Vec4,
    pub wall_a: Vec4,
    pub wall_b: Vec4,
    pub floor_a: Vec4,
    pub floor_b: Vec4,
    /// rgb: moss; w: how brightly it glows.
    pub moss: Vec4,
    /// rgb: mineral flecks.
    pub accent: Vec4,
    /// x: the even light of a lit room on the rock the character sees.
    pub light: Vec4,
}

impl Default for CaveParams {
    fn default() -> Self {
        Self {
            player: Vec4::new(0.0, 1.0, 0.0, -0.5),
            camera: Vec4::new(0.0, 10.0, 8.0, 1.5),
            wall_a: Vec4::new(0.20, 0.19, 0.26, 1.0),
            wall_b: Vec4::new(0.34, 0.30, 0.36, 1.0),
            floor_a: Vec4::new(0.30, 0.23, 0.17, 1.0),
            floor_b: Vec4::new(0.42, 0.34, 0.25, 1.0),
            moss: Vec4::new(0.16, 0.34, 0.27, 0.22),
            accent: Vec4::new(0.45, 0.95, 0.9, 1.0),
            light: Vec4::new(0.42, 0.0, 0.0, 0.0),
        }
    }
}
