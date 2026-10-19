use super::animation::ease;
use super::*;

// ---------------------------------------------------------------------------
// Effects: bolts, flashes, shockwaves and wards.

#[derive(Component)]
pub struct Bolt {
    from: Vec3,
    to: Vec3,
    delay: f32,
    t: f32,
    wobble: f32,
    accent: Accent,
}

#[derive(Component)]
pub struct Fade {
    t: f32,
    life: f32,
    grow: f32,
    base: f32,
}

fn sphere(library: &mut PropLibrary, meshes: &mut Assets<Mesh>, accent: Accent) -> Model {
    library.get(format!("fx-sphere:{accent:?}"), meshes, move || {
        let mut p = Parts {
            accent: Some(accent),
            ..default()
        };
        p.glow.blob(Vec3::ZERO, Vec3::ONE, 0, 0.0, Vec3::ONE);
        p
    })
}

fn ring(library: &mut PropLibrary, meshes: &mut Assets<Mesh>, accent: Accent) -> Model {
    library.get(format!("fx-ring:{accent:?}"), meshes, move || {
        let mut p = Parts {
            accent: Some(accent),
            ..default()
        };
        p.glow
            .torus(Vec3::ZERO, Vec3::Y, 1.0, 0.035, 32, 3, Vec3::ONE);
        p
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "effect construction receives its asset stores and trajectory together"
)]
pub fn bolt(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    from: Vec3,
    to: Vec3,
    accent: Accent,
    delay: f32,
    wobble: f32,
) {
    let model = sphere(library, meshes, accent);
    commands
        .spawn((
            Bolt {
                from,
                to,
                delay,
                t: 0.0,
                wobble,
                accent,
            },
            Transform::from_translation(from).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| {
            model.attach(p);
            p.spawn(PointLight {
                color: accent.light(),
                intensity: 12_000.0,
                range: 4.0,
                shadows_enabled: false,
                ..default()
            });
        });
}

pub fn flash(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
    size: f32,
    delay: f32,
) {
    let model = sphere(library, meshes, accent);
    commands
        .spawn((
            Fade {
                t: -delay,
                life: 0.3,
                grow: size,
                base: size * 0.2,
            },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

pub fn shockwave(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
    delay: f32,
) {
    let model = ring(library, meshes, accent);
    commands
        .spawn((
            Fade {
                t: -delay,
                life: 0.5,
                grow: 1.4,
                base: 0.15,
            },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

/// A ward: rings of light rising around a figure.
pub fn ward(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
) {
    for k in 0..3 {
        let model = ring(library, meshes, accent);
        commands
            .spawn((
                Fade {
                    t: -(k as f32) * 0.12,
                    life: 0.8,
                    grow: 0.25,
                    base: 0.5,
                },
                Transform::from_translation(at + Vec3::Y * (0.3 + k as f32 * 0.5))
                    .with_scale(Vec3::ZERO),
                Visibility::default(),
            ))
            .with_children(|p| model.attach(p));
    }
}

pub fn effects(
    mut commands: Commands,
    time: Res<Time>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bolts: Query<(Entity, &mut Bolt, &mut Transform), Without<Fade>>,
    mut fades: Query<(Entity, &mut Fade, &mut Transform), Without<Bolt>>,
) {
    let dt = time.delta_secs();
    for (e, mut bolt, mut transform) in &mut bolts {
        if bolt.delay > 0.0 {
            bolt.delay -= dt;
            continue;
        }
        bolt.t += dt / 0.32;
        let t = bolt.t.min(1.0);
        let side = (bolt.to - bolt.from).cross(Vec3::Y).normalize_or_zero();
        let arc = Vec3::Y * (t * PI).sin() * 0.25 + side * (t * TAU * 1.5).sin() * bolt.wobble;
        transform.translation = bolt.from.lerp(bolt.to, t) + arc;
        transform.scale = Vec3::splat(0.09 + 0.02 * (time.elapsed_secs() * 40.0).sin());
        if bolt.t >= 1.0 {
            commands.entity(e).despawn();
            flash(
                &mut commands,
                &mut library,
                &mut meshes,
                bolt.to,
                bolt.accent,
                0.45,
                0.0,
            );
        }
    }
    for (e, mut fade, mut transform) in &mut fades {
        fade.t += dt;
        if fade.t < 0.0 {
            continue;
        }
        let k = fade.t / fade.life;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let size = fade.base + fade.grow * ease(k);
        let thin = 1.0 - k * k;
        transform.scale = Vec3::new(size, size * thin.max(0.05), size);
        if fade.base < 0.3 && fade.grow < 1.0 {
            transform.scale = Vec3::splat(size * thin.max(0.05));
        }
    }
}

/// A ring of light that hangs in the air and fades.
fn halo(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    at: Vec3,
    accent: Accent,
    size: f32,
    delay: f32,
) {
    let model = ring(library, meshes, accent);
    commands
        .spawn((
            Fade {
                t: -delay,
                life: 0.7,
                grow: size,
                base: size * 0.4,
            },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

/// Stage a move's effects between a caster and its target (feet positions
/// and heights). Returns, when the target is struck, the delay until it
/// reacts to the blow or the spell's arrival.
#[allow(clippy::too_many_arguments)]
pub fn stage(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    action: Move,
    from: (Vec3, f32),
    to: Option<(Vec3, f32)>,
    landed: bool,
) -> Option<f32> {
    let impact = action.impact() * action.duration();
    let (caster, height) = from;
    let chest = |(at, h): (Vec3, f32)| at + Vec3::Y * h * 0.55;
    let toward = to.map_or(Vec3::Z, |(at, _)| (at - caster).normalize_or(Vec3::Z));
    let hand = caster + Vec3::Y * height * 0.7 + toward * 0.35;
    match action {
        Move::Gesture => return None,
        Move::Guard(accent) => {
            ward(commands, library, meshes, caster, accent);
            return None;
        }
        Move::Focus => {
            halo(
                commands,
                library,
                meshes,
                caster + Vec3::Y * height * 0.92,
                Accent::Teal,
                0.25,
                0.2,
            );
            halo(
                commands,
                library,
                meshes,
                caster + Vec3::Y * height * 0.92,
                Accent::Teal,
                0.25,
                0.55,
            );
            return None;
        }
        Move::Step => {
            shockwave(
                commands,
                library,
                meshes,
                caster + Vec3::Y * 0.04,
                Accent::White,
                impact,
            );
            return None;
        }
        Move::Mend => {
            ward(commands, library, meshes, caster, Accent::Teal);
            for k in 0..3 {
                flash(
                    commands,
                    library,
                    meshes,
                    caster + Vec3::Y * (0.5 + 0.12 * k as f32),
                    Accent::Teal,
                    0.18,
                    impact + k as f32 * 0.2,
                );
            }
            return None;
        }
        Move::Fear => {
            for k in 0..3 {
                halo(
                    commands,
                    library,
                    meshes,
                    caster + Vec3::Y * height * 0.85,
                    Accent::Violet,
                    0.9 + k as f32 * 0.4,
                    impact + k as f32 * 0.1,
                );
            }
        }
        _ => {}
    }
    let target = to?;
    let travel = match action {
        Move::Cast(accent) => {
            bolt(
                commands,
                library,
                meshes,
                hand,
                chest(target),
                accent,
                impact,
                0.0,
            );
            0.32
        }
        Move::Hex => {
            bolt(
                commands,
                library,
                meshes,
                hand,
                chest(target),
                Accent::Violet,
                impact,
                0.2,
            );
            0.32
        }
        Move::Breath(accent) => {
            // A stream poured from the open jaw.
            let mouth = caster + Vec3::Y * height * 0.72 + toward * 0.55;
            for k in 0..5 {
                bolt(
                    commands,
                    library,
                    meshes,
                    mouth,
                    chest(target) + Vec3::Y * (k as f32 - 2.0) * 0.06,
                    accent,
                    impact + k as f32 * 0.07,
                    0.1,
                );
            }
            0.3
        }
        Move::Gaze => {
            // The eyes kindle, and the stare crosses to the target's eyes.
            let eyes = caster + Vec3::Y * height * 0.9 + toward * 0.12;
            halo(
                commands,
                library,
                meshes,
                eyes,
                Accent::Amber,
                0.3,
                impact * 0.6,
            );
            bolt(
                commands,
                library,
                meshes,
                eyes,
                target.0 + Vec3::Y * target.1 * 0.9,
                Accent::Violet,
                impact,
                0.0,
            );
            0.25
        }
        Move::Drain => {
            // The life leaves the target and flows back into the caster.
            for k in 0..3 {
                bolt(
                    commands,
                    library,
                    meshes,
                    chest(target),
                    hand,
                    Accent::Ember,
                    impact + k as f32 * 0.12,
                    0.12,
                );
            }
            0.0
        }
        _ => 0.0,
    };
    if landed {
        let (accent, size) = match action {
            Move::Touch(accent) => (accent, 0.55),
            Move::Bite(accent) => (accent, 0.4),
            Move::Breath(accent) => (accent, 0.5),
            Move::Heavy => (Accent::Gold, 0.45),
            Move::Bash | Move::Shove => (Accent::Gold, 0.35),
            Move::Cast(_) | Move::Hex | Move::Drain | Move::Fear | Move::Gaze => {
                (Accent::White, 0.0)
            }
            _ => (Accent::White, 0.32),
        };
        if size > 0.0 {
            flash(
                commands,
                library,
                meshes,
                chest(target),
                accent,
                size,
                impact,
            );
        }
        if action == Move::Heavy {
            shockwave(
                commands,
                library,
                meshes,
                target.0 + Vec3::Y * 0.05,
                Accent::Gold,
                impact,
            );
        }
    }
    Some(impact + travel)
}
