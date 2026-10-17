//! Shared renderer setup for live play and the offline showcases.
use bevy::{
    app::PluginGroupBuilder,
    core_pipeline::fxaa::Fxaa,
    prelude::*,
    render::{
        settings::{Backends, WgpuSettings},
        RenderPlugin,
    },
};

pub(super) fn plugins() -> PluginGroupBuilder {
    let settings = platform_settings(
        WgpuSettings::default(),
        cfg!(target_os = "windows"),
        std::env::var_os("WGPU_BACKEND").is_some(),
    );
    DefaultPlugins.set(RenderPlugin {
        render_creation: settings.into(),
        ..default()
    })
}

/// Shared anti-aliasing for live play and the two procedural previews.
pub(super) fn camera_antialiasing() -> impl Bundle {
    // A single-sample target avoids the invalid DX12 software-adapter resolve.
    // Bevy's post-process FXAA keeps procedural silhouettes antialiased.
    (Msaa::Off, Fxaa::default())
}

fn platform_settings(mut settings: WgpuSettings, windows: bool, explicit: bool) -> WgpuSettings {
    // Prefer the native Windows backend. Retain Bevy's explicit backend
    // override and its normal platform defaults elsewhere.
    if windows && !explicit {
        settings.backends = Some(Backends::DX12);
    }
    settings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_camera_modes_use_single_sample_rendering_with_engine_antialiasing() {
        let mut world = World::new();
        let camera = world
            .spawn((Camera3d::default(), camera_antialiasing()))
            .id();
        assert_eq!(world.get::<Msaa>(camera).unwrap().samples(), 1,
            "avoid the invalid uninitialized multisample resolve reported by DirectX12 software rendering");
        assert!(
            world.get::<Fxaa>(camera).unwrap().enabled,
            "silhouettes still need anti-aliasing after tone mapping"
        );
    }

    #[test]
    fn windows_defaults_to_dx12_and_explicit_choices_are_preserved() {
        for (windows, explicit, expected) in [
            (true, false, Backends::DX12),
            (true, true, Backends::VULKAN),
            (false, false, Backends::VULKAN),
            (false, true, Backends::VULKAN),
        ] {
            let settings = platform_settings(
                WgpuSettings {
                    backends: Some(Backends::VULKAN),
                    ..default()
                },
                windows,
                explicit,
            );
            assert_eq!(settings.backends, Some(expected));
        }
    }
}
