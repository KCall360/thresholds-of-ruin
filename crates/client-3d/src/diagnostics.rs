//! Capture evidence after GPU readback, without an alternate input driver.
use super::*;
use bevy::render::view::screenshot::ScreenshotCaptured;

#[cfg(test)]
mod tests;

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct RenderEvidence<'w, 's> {
    game: Option<Res<'w, Game>>,
    bodies: Query<
        'w,
        's,
        (
            Entity,
            &'static ActorBody,
            &'static Anim,
            &'static GlobalTransform,
        ),
    >,
    lamps: Query<'w, 's, &'static PointLight>,
    adapter: Option<Res<'w, bevy::render::renderer::RenderAdapterInfo>>,
    camera: Query<'w, 's, (&'static Camera, &'static GlobalTransform)>,
    window: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    hover: Option<Res<'w, interact::Hover>>,
    panel: Option<Res<'w, interact::Panel>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
}

pub(super) fn capture(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut diagnostics: ResMut<Diagnostics>,
    mut exit: EventWriter<AppExit>,
    keys: Res<ButtonInput<KeyCode>>,
    evidence: RenderEvidence,
) {
    let RenderEvidence {
        game,
        bodies,
        lamps,
        adapter,
        camera,
        window,
        hover,
        panel,
        mouse,
    } = evidence;
    if diagnostics.capture_failed {
        exit.write(AppExit::error());
        return;
    }
    if let Some(limit) = diagnostics.quit_after {
        if time.elapsed_secs() > limit {
            if diagnostics.pending_shots == 0 {
                exit.write(AppExit::Success);
            } else if time.elapsed_secs() > limit + 10.0 {
                eprintln!("Screenshot readback timed out.");
                exit.write(AppExit::error());
            }
            return;
        }
    }
    // Readback can lag behind the app on software or busy GPUs. Starting
    // another capture then allocates another render target and transfer
    // buffer; retain one request until its actual completion observer runs.
    // Keep this after the exit deadline so stalled readback still fails.
    if diagnostics.pending_shots != 0 {
        return;
    }
    let Some(dir) = diagnostics.capture.clone() else {
        return;
    };
    if time.elapsed_secs() < diagnostics.next_shot.max(3.0) {
        return;
    }
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("Cannot create capture directory: {error}");
        diagnostics.capture_failed = true;
        return;
    }
    diagnostics.next_shot = time.elapsed_secs() + diagnostics.capture_every.unwrap_or(4.0);
    diagnostics.shots += 1;
    diagnostics.pending_shots += 1;
    let path = dir.join(format!("shot-{:02}.png", diagnostics.shots));
    let mut pressed: Vec<_> = keys.get_pressed().map(|key| format!("{key:?}")).collect();
    pressed.sort();
    let project_world = |point| {
        let window = window.single().ok()?;
        let (camera, eye) = camera.single().ok()?;
        let point = camera.world_to_viewport(eye, point).ok()?;
        Some([point.x / window.width(), point.y / window.height()])
    };
    let mut actors: Vec<_> = bodies
        .iter()
        .map(|(entity, body, anim, transform)| {
            let target = game.as_ref().and_then(|game| {
                game.actors
                    .iter()
                    .find_map(|(target, body)| (*body == entity).then_some(*target))
            });
            let is_self = target.is_some_and(|id| {
                game.as_ref()
                    .and_then(|game| game.view.as_ref())
                    .is_some_and(|view| {
                        view.actors
                            .iter()
                            .any(|actor| actor.id == id && actor.is_self)
                    })
            });
            serde_json::json!({
                "target": target, "is_self": is_self, "hostile": body.hostile,
                "world": transform.translation().to_array(),
                "screen": project_world(transform.translation() + Vec3::Y * body.height * 0.5),
                "motion": anim.action.map(|(action, _)| format!("{action:?}")),
                "name": body.name, "model": format!("{:?}", body.model),
                "appearance_seed": body.appearance_seed, "appearance_key": body.appearance_key,
            })
        })
        .collect();
    actors.sort_by_key(|actor| actor.to_string());
    let view = game.as_ref().and_then(|game| game.view.as_ref());
    let project = |position| {
        let game = game.as_ref()?;
        project_world(stand(ivec(position) + game.offset) + Vec3::Y * 0.15)
    };
    let mut pressed_mouse: Vec<_> = mouse
        .get_pressed()
        .map(|button| format!("{button:?}"))
        .collect();
    pressed_mouse.sort();
    let report = serde_json::json!({
        "type": if view.is_some() { "3d_frame" } else { "3d_capture" },
        "gpu_readback": true, "png": path, "keys": pressed, "actors": actors,
        "captures_in_flight": diagnostics.pending_shots,
        "mouse": pressed_mouse,
        "status": game.as_ref().map(|game| &game.status),
        "world_feet": game.as_ref().map(|game| game.feet_world.to_array()),
        "foot_cell": view.and_then(|view| view.visible.iter().find(|cell| cell.position == view.feet).map(|cell| &cell.key)),
        "doors": view.map(|view| view.visible.iter().filter_map(|cell| cell.door.as_ref()).map(|door| serde_json::json!({
            "id": door.id, "open": door.open, "reachable": door.reachable,
        })).collect::<Vec<_>>()),
        "panel_open": panel.is_some_and(|panel| panel.open),
        "hover": { "title": hover.as_ref().map(|hover| &hover.title),
            "cell": hover.as_ref().and_then(|hover| hover.cell).map(|cell| {
                let local = cell - game.as_ref().map_or(IVec3::ZERO, |game| game.offset);
                tor_protocol::Position { x: local.x, y: local.y, z: local.z }
            }) },
        "renderer": adapter.as_ref().map(|adapter| serde_json::json!({
            "backend": format!("{:?}", adapter.backend), "adapter": adapter.name,
            "device_type": format!("{:?}", adapter.device_type),
            "driver": adapter.driver, "driver_info": adapter.driver_info,
        })),
        "tick": view.map(|view| view.tick.to_string()),
        "revision": view.map(|view| view.revision.to_string()),
        "shown": view.map(|view| &view.shown),
        "has_control": view.is_some_and(|view| view.has_control),
        "can_act": view.is_some_and(|view| view.can_act),
        "visible_cells": view.map_or(0, |view| view.visible.len()),
        "visible_keys": view.map(|view| view.visible.iter().map(|cell| &cell.key).collect::<Vec<_>>()),
        "remembered_terrain": view.map(|view| view.terrain().filter(|c| !c.seen).map(|cell|
            serde_json::json!({"key":cell.key,"position":cell.position,"lit":cell.lit})
        ).collect::<Vec<_>>()),
        "lit_cells": view.map_or(0, |view| view.visible.iter().filter(|cell| cell.lit).count()),
        "active_point_lights": lamps.iter().filter(|light| light.intensity > 0.0).count(),
        "fatal": game.as_ref().is_some_and(|game| game.fatal),
        "busy": game.as_ref().is_some_and(|game| game.busy),
        "inventory": view.map(|view| view.inventory.iter().map(|item| serde_json::json!({
            "name": item.name, "model": format!("{:?}", item.model), "quantity": item.quantity.to_string(),
        })).collect::<Vec<_>>()),
        "ground_items": view.map(|view| view.items.iter().map(|item| serde_json::json!({
            "name": item.name, "model": format!("{:?}", item.model), "quantity": item.quantity.to_string(),
            "position": item.position, "screen": project(item.position),
            "cell_key": view.visible.iter().find(|cell| cell.position == item.position).map(|cell| &cell.key),
        })).collect::<Vec<_>>()),
    });
    let mut save = save_to_disk(path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |trigger: Trigger<ScreenshotCaptured>, mut diagnostics: ResMut<Diagnostics>| {
            let luminance = scene_luminance(&trigger.event().0);
            save(trigger);
            diagnostics.pending_shots = diagnostics.pending_shots.saturating_sub(1);
            if path.is_file() {
                let mut report = report.clone();
                report["scene_luminance"] = luminance.into();
                println!("{report}");
            } else {
                eprintln!("Screenshot was not saved: {}", path.display());
                diagnostics.capture_failed = true;
            }
        },
    );
}

/// Sample the actual GPU image's central scene, excluding the HUD margins.
fn scene_luminance(image: &Image) -> Option<f32> {
    let size = image.texture_descriptor.size;
    let (mut sum, mut count) = (0.0, 0);
    for y in (size.height / 4..size.height * 3 / 4).step_by(8) {
        for x in (size.width / 4..size.width * 3 / 4).step_by(8) {
            let color = image.get_color_at(x, y).ok()?.to_linear();
            sum += color.red * 0.2126 + color.green * 0.7152 + color.blue * 0.0722;
            count += 1;
        }
    }
    (count > 0).then(|| sum / count as f32)
}
