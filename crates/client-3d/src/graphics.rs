//! Shared renderer setup for live play and the offline showcases.
use bevy::{
    app::PluginGroupBuilder,
    core_pipeline::fxaa::Fxaa,
    prelude::*,
    render::{
        render_resource::Maintain,
        renderer::{RenderAdapterInfo, RenderDevice},
        settings::{Backends, WgpuSettings},
        Render, RenderApp, RenderPlugin, RenderSet,
    },
};
use wgpu_types::{Backend, DeviceType};

pub(super) fn plugins() -> PluginGroupBuilder {
    let settings = platform_settings(
        WgpuSettings::default(),
        cfg!(target_os = "windows"),
        std::env::var_os("WGPU_BACKEND").is_some(),
    );
    DefaultPlugins
        .set(RenderPlugin {
            render_creation: settings.into(),
            ..default()
        })
        .add(SoftwareFramePacingPlugin)
}

// Software DX12 can submit frames faster than it executes them, retaining
// per-frame bind groups until the adapter runs out of resources. Hardware
// adapters and other backends retain Bevy's normal pipelined rendering.
struct SoftwareFramePacingPlugin;
impl Plugin for SoftwareFramePacingPlugin {
    fn build(&self, app: &mut App) {
        app.sub_app_mut(RenderApp)
            .add_systems(Render, wait_for_software_frame.in_set(RenderSet::Cleanup));
    }
}

fn needs_frame_wait(device_type: DeviceType, backend: Backend) -> bool {
    device_type == DeviceType::Cpu && backend == Backend::Dx12
}

fn wait_for_software_frame(device: Res<RenderDevice>, adapter: Res<RenderAdapterInfo>) {
    if needs_frame_wait(adapter.device_type, adapter.backend) {
        let _ = device.poll(Maintain::Wait);
    }
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
    fn software_frame_wait_applies_only_to_cpu_dx12() {
        for device in [
            DeviceType::Cpu,
            DeviceType::IntegratedGpu,
            DeviceType::DiscreteGpu,
            DeviceType::VirtualGpu,
            DeviceType::Other,
        ] {
            for backend in [
                Backend::Dx12,
                Backend::Vulkan,
                Backend::Gl,
                Backend::Metal,
                Backend::BrowserWebGpu,
                Backend::Empty,
            ] {
                assert_eq!(
                    needs_frame_wait(device, backend),
                    device == DeviceType::Cpu && backend == Backend::Dx12
                );
            }
        }
    }

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
