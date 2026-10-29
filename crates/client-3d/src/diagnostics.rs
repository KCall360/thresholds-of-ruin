//! Capture evidence after GPU readback, without an alternate input driver.
use super::*;
use bevy::render::view::screenshot::ScreenshotCaptured;

#[cfg(test)]
mod tests;

type ConfirmationButton = (
    Entity,
    &'static crate::progression::Control,
    &'static ComputedNode,
    &'static GlobalTransform,
    Option<&'static bevy::ui::CalculatedClip>,
    &'static InheritedVisibility,
);

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
            Option<&'static creatures::Rig>,
        ),
    >,
    lamps: Query<'w, 's, &'static PointLight>,
    pending_models: Query<'w, 's, (), With<creatures::loading::PendingModel>>,
    failed_models: Query<'w, 's, (), With<creatures::loading::LoadFailed>>,
    adapter: Option<Res<'w, bevy::render::renderer::RenderAdapterInfo>>,
    camera: Query<
        'w,
        's,
        (&'static Camera, &'static GlobalTransform),
        crate::portrait::ExplorationCamera,
    >,
    window: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    hover: Option<Res<'w, interact::Hover>>,
    panel: Option<Res<'w, interact::Panel>>,
    minimap: Query<'w, 's, &'static Visibility, With<minimap::MiniMapFrame>>,
    hud_buttons: Query<
        'w,
        's,
        (
            &'static crate::hud::Action,
            &'static ComputedNode,
            &'static GlobalTransform,
        ),
    >,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    inventory_controls: Query<
        'w,
        's,
        (
            &'static crate::inventory::Control,
            &'static ComputedNode,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    panel_buttons: Query<
        'w,
        's,
        (
            &'static interact::PanelButton,
            &'static ComputedNode,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    panel_scroll:
        Query<'w, 's, (&'static ComputedNode, &'static GlobalTransform), With<inventory::Scroll>>,
    portrait: Option<Res<'w, crate::portrait::Portrait>>,
    progression: Option<Res<'w, crate::progression::State>>,
    confirmation_buttons: Query<'w, 's, ConfirmationButton>,
    control_text: Query<'w, 's, (&'static Text, &'static ChildOf)>,
    character_sheet: Query<
        'w,
        's,
        (&'static ComputedNode, &'static GlobalTransform),
        With<crate::character::Root>,
    >,
    map_route: Option<Res<'w, crate::map_journal::route::State>>,
    journal_map: Option<Res<'w, crate::map_journal::Map>>,
    journal_picture: Query<
        'w,
        's,
        (
            &'static ComputedNode,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
        With<crate::map_journal::Picture>,
    >,
    portrait_picture: Query<
        'w,
        's,
        (&'static ComputedNode, &'static GlobalTransform),
        With<crate::portrait::Picture>,
    >,
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
        pending_models,
        failed_models,
        adapter,
        camera,
        window,
        hover,
        panel,
        minimap,
        hud_buttons,
        mouse,
        inventory_controls,
        panel_buttons,
        panel_scroll,
        portrait,
        progression,
        confirmation_buttons,
        control_text,
        character_sheet,
        journal_picture,
        map_route,
        journal_map,
        portrait_picture,
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
        .map(|(entity, body, anim, transform, rig)| {
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
                "model_ready": !pending_models.contains(entity) && !failed_models.contains(entity),
                "presented_plan": rig.map(|rig| format!("{:?}", rig.plan)),
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
    let screen_text = game
        .as_ref()
        .filter(|game| game.screen.is_some())
        .map(|game| crate::screen::lines(game));
    let typed = game
        .as_ref()
        .filter(|game| game.screen.is_some())
        .map(|game| game.typed.clone());
    let window_size = window.single().map_or(Vec2::ONE, |window| {
        Vec2::new(
            window.physical_width() as f32,
            window.physical_height() as f32,
        )
    });
    let bounds = |node: &ComputedNode, transform: &GlobalTransform| {
        let center = transform.translation().truncate();
        let min = (center - node.size() * 0.5) / window_size;
        let max = (center + node.size() * 0.5) / window_size;
        [min.x, min.y, max.x, max.y]
    };
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
        "panel_open": panel.as_ref().is_some_and(|panel| panel.open),
        "map_journal":journal_picture.single().ok().map(|(node,transform,visible)|serde_json::json!({"visible":visible.get(),"picture_bounds":bounds(node,transform),"route":crate::map_journal::route::diagnostics(map_route.as_deref(),game.as_deref(),journal_map.as_deref())})),
        "portrait":portrait.as_ref().map(|portrait|serde_json::json!({
            "active":portrait.active,"turn":portrait.turn,"texture_size":portrait.size.to_array(),"mesh_parts":portrait.parts(),
            "target":portrait.owner.and_then(|owner|game.as_ref()?.actors.iter().find_map(|(target,entity)|(*entity==owner).then_some(*target))),
            "pending":portrait.owner.is_some_and(|owner|pending_models.contains(owner)),
            "bounds":portrait_picture.single().ok().map(|(node,transform)|bounds(node,transform)),
        })),
        "panel_viewport":panel_scroll.single().ok().map(|(node,transform)|bounds(node,transform)),
        "panel_buttons":panel_buttons.iter().filter(|(_,_,_,visible)|visible.get()).map(|(action,node,transform,_)|serde_json::json!({
            "target":inventory::item_target(*action),
            "operation":match action {interact::PanelButton::Use(_,operation)=>Some(format!("{operation:?}")),_=>None},
            "bounds":bounds(node,transform),
        })).collect::<Vec<_>>(),
        "minimap_open": minimap.iter().any(|visibility| *visibility != Visibility::Hidden),
        "inventory_browser": panel.as_ref().filter(|panel|panel.open).and_then(|panel|game.as_ref().filter(|game|crate::inventory::grid(game)).map(|game|serde_json::json!({
            "page":format!("{:?}",panel.page),
            "comparison_slot":game.view.as_ref().and_then(|view|crate::equipment::comparison_slot(view,panel)),
            "query":panel.browser.query(),"filter":format!("{:?}",panel.browser.filter()),"selected":panel.browser.selected(),"search_focused":panel.search_focused,
            "shown":game.view.as_ref().map(|view|panel.browser.carried(view).map(|item|item.id).collect::<Vec<_>>()),
            "controls":inventory_controls.iter().filter(|(_,_,_,visible)|visible.get()).map(|(control,node,transform,_)|{
                let size=window.single().map_or(Vec2::ONE,|w|Vec2::new(w.physical_width() as f32,w.physical_height() as f32));
                let center=transform.translation();serde_json::json!({"control":format!("{control:?}"),"target":match control{crate::inventory::Control::Select(id)=>Some(*id),_=>None},"screen":[center.x/size.x,center.y/size.y],"size":node.size().to_array()})
            }).collect::<Vec<_>>()
        }))),
        "hud": game.as_ref().map(|game| serde_json::json!({
            "readiness": crate::hud::readiness(game),
            "health": game.view.as_ref().and_then(|view| view.hp),
            "buttons": hud_buttons.iter().map(|(action, node, transform)| {
                let center = transform.translation();
                let size = window.single().map_or(Vec2::ONE, |w| Vec2::new(w.physical_width() as f32, w.physical_height() as f32));
                serde_json::json!({ "action": format!("{action:?}"), "screen": [center.x / size.x, center.y / size.y], "size": node.size().to_array() })
            }).collect::<Vec<_>>()
        })),
        // Pack actions in number-key order, as "row: action".
        "panel_actions": panel.as_ref().map(|panel| panel.actions.iter().map(|(label, _)| label.clone()).collect::<Vec<_>>()),
        "panel_item_targets": panel.as_ref().map(|panel|panel.actions.iter().map(|(_,action)|crate::inventory::item_target(*action)).collect::<Vec<_>>()),
        "advancement": game.as_ref().filter(|game| game.screen == Some(crate::screen::Screen::Advancement)).map(|game| serde_json::json!({
            "lines": game.advancement, "typed": game.typed,
            "pane_bounds": character_sheet.single().ok().map(|(node, transform)| {
                let size = window.single().map_or(Vec2::ONE, |window| Vec2::new(window.physical_width() as f32, window.physical_height() as f32));
                let rect = Rect::from_center_size(transform.translation().truncate(), node.size());
                [rect.min.x / size.x, rect.min.y / size.y, rect.max.x / size.x, rect.max.y / size.y]
            }),
            "choices": progression.as_ref().map(|state| {
                let mut choices = crate::progression::diagnostics(game, state);
                choices["confirm_button"] = confirmation_buttons.iter().find(|(_, control, _, _, _, visible)| control.is_confirmation() && visible.get()).map_or(serde_json::Value::Null, |(entity, _, node, transform, clip, _)| {
                    let bounds = Rect::from_center_size(transform.translation().truncate(), node.size());
                    let size = window.single().map_or(Vec2::ONE, |window| Vec2::new(window.physical_width() as f32, window.physical_height() as f32));
                    let clip = clip.map_or(Rect::from_corners(Vec2::ZERO, size), |clip| clip.clip);
                    let label = control_text.iter().find_map(|(text, parent)| (parent.parent() == entity).then_some(text.0.as_str()));
                    serde_json::json!({"label":label,"bounds":[bounds.min.x,bounds.min.y,bounds.max.x,bounds.max.y],"clip":[clip.min.x,clip.min.y,clip.max.x,clip.max.y]})
                });
                choices
            }),
        })),
        "screen": game.as_ref().and_then(|game| game.screen).map(|screen| format!("{screen:?}")),
        // The open screen as drawn, and the line being typed on it.
        "screen_text": screen_text,
        "typed": typed,
        "aiming": game.as_ref().and_then(|game| game.aiming.as_ref()).map(|aim| format!("{aim:?}")),
        "projectile": game.as_ref().and_then(|game| game.projectile.as_ref()).map(|prompt| serde_json::json!({
            "title": prompt.title, "rows": prompt.rows, "choosing": prompt.choosing, "aiming": prompt.aiming,
        })),
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
        "pending_models": pending_models.iter().count(),
        "failed_models": failed_models.iter().count(),
        "fatal": game.as_ref().is_some_and(|game| game.fatal),
        "busy": game.as_ref().is_some_and(|game| game.busy),
        "inventory": view.map(|view| view.inventory.iter().map(|item| serde_json::json!({
            "id": item.id, "name": item.name, "model": format!("{:?}", item.model), "quantity": item.quantity.to_string(),
            "equipped":item.equipped,
        })).collect::<Vec<_>>()),
        "equipment_slots":view.map(|view|view.equipment.slots.iter().enumerate().map(|(index,slot)|serde_json::json!({"index":index,"name":slot.name,"occupant":u16::try_from(index).ok().and_then(|index|view.equipment.occupant(index))})).collect::<Vec<_>>()),
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
