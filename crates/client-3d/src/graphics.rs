//! Shared renderer setup for live play and the offline showcases.
use bevy::{
    app::PluginGroupBuilder,
    core_pipeline::fxaa::Fxaa,
    pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel},
    prelude::*,
    render::{
        render_resource::Maintain,
        renderer::{RenderAdapterInfo, RenderDevice},
        settings::{Backends, Dx12Compiler, WgpuSettings},
        Render, RenderApp, RenderPlugin, RenderSet,
    },
};
use std::path::{Path, PathBuf};
use wgpu_types::{Backend, DeviceType};

pub(super) fn plugins() -> PluginGroupBuilder {
    let requested_compiler =
        std::env::var_os("WGPU_DX12_COMPILER").map(|value| value.to_string_lossy().into_owned());
    let settings = platform_settings(
        WgpuSettings::default(),
        cfg!(target_os = "windows"),
        std::env::var_os("WGPU_BACKEND").is_some(),
    );
    let settings = adjacent_shader_compiler(
        settings,
        cfg!(target_os = "windows"),
        requested_compiler.is_some(),
    );
    let compiler_log = ShaderCompilerLog {
        requested: requested_compiler,
        selected: settings.dx12_shader_compiler.clone(),
    };
    DefaultPlugins
        .set(RenderPlugin {
            render_creation: settings.into(),
            // A ready world must not draw only its UI while asynchronously
            // compiling the material pipelines needed by its meshes.
            synchronous_pipeline_compilation: true,
            ..default()
        })
        .add(SoftwareFramePacingPlugin)
        .add(compiler_log)
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

/// Log after DefaultPlugins installs its subscriber, rather than while the
/// plugin group is still being assembled.
struct ShaderCompilerLog {
    requested: Option<String>,
    selected: Dx12Compiler,
}

impl Plugin for ShaderCompilerLog {
    fn build(&self, _app: &mut App) {
        if cfg!(target_os = "windows") {
            info!(
                "DX12 shader compiler requested={} selected={:?}",
                self.requested.as_deref().unwrap_or("auto"),
                self.selected,
            );
        }
    }
}

fn adjacent_shader_compiler(
    mut settings: WgpuSettings,
    windows: bool,
    explicit: bool,
) -> WgpuSettings {
    if !windows || explicit || matches!(settings.dx12_shader_compiler, Dx12Compiler::StaticDxc) {
        return settings;
    }
    // Compiler discovery must not depend on the launcher's working directory.
    // A distribution without the optional pair retains the built-in compiler.
    settings.dx12_shader_compiler = Dx12Compiler::Fxc;
    // FXC compiles the shadowed PBR shader into code several times slower on
    // the GPU (a crowd's opaque pass took 10 ms rather than 2), so a pair
    // shipped beside the executable wins, then the newest installed Windows
    // SDK's; FXC remains only where neither exists.
    let mut directories: Vec<PathBuf> = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
        .into_iter()
        .collect();
    if let Some(programs) = std::env::var_os("ProgramFiles(x86)") {
        directories.extend(sdk_compiler_directories(
            &Path::new(&programs).join(r"Windows Kits\10\bin"),
        ));
    }
    if let Some((compiler, validator)) = compiler_pair(&directories) {
        settings.dx12_shader_compiler = Dx12Compiler::DynamicDxc {
            dxc_path: compiler.to_string_lossy().into_owned(),
            dxil_path: validator.to_string_lossy().into_owned(),
        };
    }
    settings
}

/// The x64 tool directories of a Windows SDK `bin` directory, newest first.
fn sdk_compiler_directories(bin: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(bin) else {
        return Vec::new();
    };
    let mut versions: Vec<(Vec<u32>, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let version = name
                .split('.')
                .map(str::parse)
                .collect::<Result<Vec<u32>, _>>()
                .ok()?;
            Some((version, entry.path().join("x64")))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions
        .into_iter()
        .map(|(_, directory)| directory)
        .collect()
}

/// The first directory holding both DXC libraries, as compiler and validator.
fn compiler_pair(directories: &[PathBuf]) -> Option<(PathBuf, PathBuf)> {
    directories.iter().find_map(|directory| {
        let compiler = directory.join("dxcompiler.dll");
        let validator = directory.join("dxil.dll");
        (compiler.is_file() && validator.is_file()).then_some((compiler, validator))
    })
}

/// Shared silhouette and contact shading for live play and both previews.
pub(super) fn camera_antialiasing() -> impl Bundle {
    // A single-sample target avoids the invalid DX12 software-adapter resolve.
    // Bevy's post-process FXAA keeps procedural silhouettes antialiased.
    // Occlusion restores depth at collars, overlapping limbs, armour seams and
    // foot contacts without darkening the lit faces or washing out their colour.
    // The engine adds depth/normal prepasses and skips SSAO on unsupported GPUs.
    (
        Msaa::Off,
        Fxaa::default(),
        ScreenSpaceAmbientOcclusion {
            quality_level: ScreenSpaceAmbientOcclusionQualityLevel::High,
            // Match the small gaps in our roughly human-scale procedural cast
            // rather than treating narrow garment layers as thick solids.
            constant_object_thickness: 0.12,
        },
    )
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
    fn the_first_complete_dxc_pair_is_chosen_from_the_newest_sdk() {
        let root = std::env::current_dir()
            .unwrap()
            .join("target/test-dxc-discovery");
        let _ = std::fs::remove_dir_all(&root);
        let bin = root.join("bin");
        for (version, files) in [
            ("10.0.17134.0", &["dxcompiler.dll", "dxil.dll"][..]),
            ("10.0.26100.0", &["dxcompiler.dll", "dxil.dll"][..]),
            // Newest, but only half a pair.
            ("10.0.30000.0", &["dxcompiler.dll"][..]),
        ] {
            let directory = bin.join(version).join("x64");
            std::fs::create_dir_all(&directory).unwrap();
            for file in files {
                std::fs::write(directory.join(file), b"").unwrap();
            }
        }
        std::fs::create_dir_all(bin.join("x64")).unwrap();
        let directories = sdk_compiler_directories(&bin);
        assert_eq!(
            directories,
            ["10.0.30000.0", "10.0.26100.0", "10.0.17134.0"].map(|v| bin.join(v).join("x64")),
            "unversioned tool directories are not SDK versions"
        );
        let (compiler, validator) = compiler_pair(&directories).unwrap();
        assert_eq!(compiler, bin.join("10.0.26100.0/x64/dxcompiler.dll"));
        assert_eq!(validator, bin.join("10.0.26100.0/x64/dxil.dll"));
        let beside = root.join("beside");
        std::fs::create_dir_all(&beside).unwrap();
        for file in ["dxcompiler.dll", "dxil.dll"] {
            std::fs::write(beside.join(file), b"").unwrap();
        }
        let mut preferred = vec![beside.clone()];
        preferred.extend(directories);
        assert_eq!(
            compiler_pair(&preferred).unwrap().0,
            beside.join("dxcompiler.dll")
        );
        assert!(compiler_pair(&[root.join("missing")]).is_none());
        assert!(sdk_compiler_directories(&root.join("missing")).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
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
