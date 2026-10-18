//! Camera-relative controls and frame interpolation.
use super::*;

#[cfg(test)]
mod tests;

pub(super) fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: EventReader<MouseWheel>,
    time: Res<Time>,
    net: Res<Net>,
    mut game: ResMut<Game>,
    mut rig: ResMut<CameraRig>,
) {
    if keys.just_pressed(KeyCode::KeyQ) {
        rig.facing -= 1;
    }
    if keys.just_pressed(KeyCode::KeyE) {
        rig.facing += 1;
    }
    for event in wheel.read() {
        rig.distance = (rig.distance - event.y * 0.8).clamp(4.0, 32.0);
    }
    game.since_send += time.delta_secs();
    if game.busy || game.fatal {
        return;
    }
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let pressed = |codes: &[KeyCode]| keys.any_pressed(codes.iter().copied());
    let fresh = |codes: &[KeyCode]| keys.any_just_pressed(codes.iter().copied());

    let command = if fresh(&[KeyCode::Tab]) {
        Some(Cmd::Control)
    } else if fresh(&[KeyCode::KeyR, KeyCode::PageUp]) || (shift && fresh(&[KeyCode::Comma])) {
        Some(Cmd::Move(Direction::Up))
    } else if fresh(&[KeyCode::KeyF, KeyCode::PageDown]) || (shift && fresh(&[KeyCode::Period])) {
        Some(Cmd::Move(Direction::Down))
    } else {
        // A screen-relative heading from held keys: up is away from the camera.
        let numpad = [
            (KeyCode::Numpad8, (0, 1)),
            (KeyCode::Numpad9, (1, 1)),
            (KeyCode::Numpad6, (1, 0)),
            (KeyCode::Numpad3, (1, -1)),
            (KeyCode::Numpad2, (0, -1)),
            (KeyCode::Numpad1, (-1, -1)),
            (KeyCode::Numpad4, (-1, 0)),
            (KeyCode::Numpad7, (-1, 1)),
        ];
        let mut heading = numpad
            .iter()
            .find(|(k, _)| keys.pressed(*k))
            .map(|(_, d)| *d);
        if heading.is_none() {
            let x = i32::from(pressed(&[KeyCode::KeyD, KeyCode::ArrowRight]))
                - i32::from(pressed(&[KeyCode::KeyA, KeyCode::ArrowLeft]));
            let y = i32::from(pressed(&[KeyCode::KeyW, KeyCode::ArrowUp]))
                - i32::from(pressed(&[KeyCode::KeyS, KeyCode::ArrowDown]));
            heading = (x, y).ne(&(0, 0)).then_some((x, y));
        }
        let movement_keys = [
            KeyCode::KeyW,
            KeyCode::KeyA,
            KeyCode::KeyS,
            KeyCode::KeyD,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Numpad1,
            KeyCode::Numpad2,
            KeyCode::Numpad3,
            KeyCode::Numpad4,
            KeyCode::Numpad6,
            KeyCode::Numpad7,
            KeyCode::Numpad8,
            KeyCode::Numpad9,
        ];
        // A tap acts at once; a held key repeats at a walking pace. A short
        // grace lets two keys pressed together make one diagonal step.
        let tapped = fresh(&movement_keys);
        let repeat = game.since_send > 0.16;
        match heading {
            Some((x, y)) if tapped || repeat => {
                let local = [
                    (0, 1),
                    (1, 1),
                    (1, 0),
                    (1, -1),
                    (0, -1),
                    (-1, -1),
                    (-1, 0),
                    (-1, 1),
                ]
                .iter()
                .position(|d| *d == (x, y))
                .unwrap_or(0) as i32;
                Some(Cmd::Move(
                    COMPASS[(local + rig.facing).rem_euclid(8) as usize],
                ))
            }
            _ if fresh(&[KeyCode::Space, KeyCode::Numpad5])
                || (!shift && fresh(&[KeyCode::Period])) =>
            {
                Some(Cmd::Wait)
            }
            _ => None,
        }
    };
    if let Some(command) = command {
        game.send(&net, command);
    }
}

pub(super) fn rig_camera(
    time: Res<Time>,
    mut game: ResMut<Game>,
    mut rig: ResMut<CameraRig>,
    mut camera: Query<&mut Transform, (With<Camera3d>, Without<Torch>)>,
    mut torch: Query<&mut Transform, (With<Torch>, Without<Camera3d>)>,
) {
    let dt = time.delta_secs();
    let player = stand(game.feet_world) + Vec3::Y * 0.65;
    let smooth = 1.0 - (-dt * 7.0).exp();
    if game.snap_camera {
        rig.focus = player;
        game.snap_camera = false;
    } else {
        rig.focus = rig.focus.lerp(player, smooth);
    }
    let goal_yaw = rig.facing as f32 * std::f32::consts::FRAC_PI_4;
    rig.yaw += (goal_yaw - rig.yaw) * (1.0 - (-dt * 9.0).exp());
    // Facing north at yaw 0: the camera sits south of the player (+z).
    let forward = Vec3::new(rig.yaw.sin(), 0.0, -rig.yaw.cos());
    let pitch = 0.95f32;
    let eye =
        rig.focus - forward * rig.distance * pitch.cos() + Vec3::Y * rig.distance * pitch.sin();
    if let Ok(mut transform) = camera.single_mut() {
        *transform = Transform::from_translation(eye).looking_at(rig.focus, Vec3::Y);
    }
    if let Ok(mut transform) = torch.single_mut() {
        transform.translation = rig.focus + Vec3::Y * 1.4;
    }
}

/// Tell the cave shader where the player and camera are, for the cutaway.
pub(super) fn update_cave_params(
    rig: Res<CameraRig>,
    palette: Res<Palette>,
    mut materials: ResMut<Assets<CaveMaterial>>,
    camera: Query<&Transform, With<Camera3d>>,
) {
    let Ok(eye) = camera.single() else {
        return;
    };
    let floor = rig.focus.y - 0.65;
    let player = (rig.focus + Vec3::Y * 0.1).extend(floor);
    let camera = eye.translation.extend(1.55);
    let Some(material) = materials.get(&palette.cave_mat) else {
        return;
    };
    // In Bevy 0.16, even an unchanged get_mut emits AssetEvent::Modified,
    // causing the renderer to recreate this material's GPU bind group.
    if material.extension.params.player == player && material.extension.params.camera == camera {
        return;
    }
    if let Some(material) = materials.get_mut(&palette.cave_mat) {
        material.extension.params.player = player;
        material.extension.params.camera = camera;
    }
}

pub(super) fn follow_actors(
    time: Res<Time>,
    mut bodies: Query<(&ActorBody, &mut Transform)>,
    mut labels: Query<(&mut Node, &mut Visibility), With<Label>>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    let smooth = 1.0 - (-time.delta_secs() * 12.0).exp();
    let Ok((camera, eye)) = camera.single() else {
        return;
    };
    for (body, mut transform) in &mut bodies {
        let step = body.goal - transform.translation;
        transform.translation += step * smooth;
        if let Ok((mut node, mut visibility)) = labels.get_mut(body.label) {
            let head = transform.translation + Vec3::Y * (body.height + 0.15);
            match camera.world_to_viewport(eye, head) {
                Ok(at) if !body.name.is_empty() => {
                    node.left = Val::Px(at.x - body.name.len() as f32 * 3.6);
                    node.top = Val::Px(at.y - 10.0);
                    *visibility = Visibility::Visible;
                }
                _ => *visibility = Visibility::Hidden,
            }
        }
    }
}

pub(super) fn torch_flicker(
    time: Res<Time>,
    mut game: ResMut<Game>,
    mut torch: Query<&mut PointLight, With<Torch>>,
) {
    let t = time.elapsed_secs();
    if game.hurt > 0.0 {
        game.hurt = (game.hurt - time.delta_secs()).max(0.0);
    }
    let red = game.hurt / 0.35;
    let flicker = 1.0 + 0.06 * (t * 11.0).sin() + 0.04 * (t * 23.7).sin() + 0.03 * (t * 5.3).cos();
    if let Ok(mut light) = torch.single_mut() {
        let lit = game
            .view
            .as_ref()
            .is_some_and(|view| lighting::at(view, view.feet));
        light.intensity = if lit {
            260_000.0 * flicker * (1.0 + red)
        } else {
            0.0
        };
        light.color = Color::srgb(1.0, 0.72 * (1.0 - red * 0.8), 0.42 * (1.0 - red * 0.8));
    }
}
