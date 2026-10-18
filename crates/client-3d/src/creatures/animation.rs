use super::*;

// ---------------------------------------------------------------------------
// Posing.

pub(super) fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rise from 0 to 1 over [a, b], hold, then fall back over [c, d].
fn envelope(t: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    if t < a || t > d {
        0.0
    } else if t < b {
        ease((t - a) / (b - a))
    } else if t < c {
        1.0
    } else {
        1.0 - ease((t - c) / (d - c))
    }
}

fn fwd(a: f32) -> Quat {
    Quat::from_rotation_x(-a)
}

/// Tip a joint that points up or ahead (a torso, a head, a neck, a beast's
/// body) forward and down by `a`; negative rears it back. `fwd` is the same
/// motion for limbs, which hang down from their joints.
fn bow(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

/// Rotations and offsets for one frame, keyed by joint.
struct Pose {
    rot: [Quat; JOINTS],
    off: [Vec3; JOINTS],
}

impl Pose {
    fn new() -> Self {
        Self {
            rot: [Quat::IDENTITY; JOINTS],
            off: [Vec3::ZERO; JOINTS],
        }
    }
    fn r(&mut self, j: J, q: Quat) {
        self.rot[j as usize] *= q;
    }
    fn o(&mut self, j: J, v: Vec3) {
        self.off[j as usize] += v;
    }
}

/// Advance timers and pose every creature.
pub fn animate(
    time: Res<Time>,
    bored: Option<Res<Boredom>>,
    mut rigs: Query<(&Rig, &mut Anim, &GlobalTransform)>,
    mut joints: Query<&mut Transform, Without<Rig>>,
) {
    let dt = time.delta_secs().min(0.05);
    // Left alone for a minute, everyone starts to amuse themselves.
    let fun = bored.map_or(0.0, |b| ease((b.0 - BORED_AFTER) / 3.0));
    for (rig, mut anim, global) in &mut rigs {
        let at = global.translation();
        let first = anim.time == 0.0;
        let moved = (at - anim.last).with_y(0.0);
        anim.last = at;
        // The dead lie still, and a body just placed hasn't moved at all.
        // A jump of more than half a cell in one frame is a placement, not a walk.
        let step = if first || anim.dead.is_some() || moved.length() > 0.5 {
            0.0
        } else {
            moved.length()
        };
        let target_speed = (step / dt.max(1e-4) / (4.0 * rig.scale.max(0.2))).clamp(0.0, 1.0);
        anim.speed += (target_speed - anim.speed) * (1.0 - (-dt * 10.0).exp());
        anim.walk += step * TAU * 0.9 * rig.stride / rig.scale.max(0.2);
        if step > 0.002 && anim.action.is_none() {
            anim.target_yaw = Some(moved.x.atan2(moved.z));
        }
        if let Some(goal) = anim.target_yaw {
            let mut d = (goal - anim.yaw) % TAU;
            if d > PI {
                d -= TAU;
            } else if d < -PI {
                d += TAU;
            }
            anim.yaw += d * (1.0 - (-dt * 12.0).exp());
        }
        anim.time += dt;
        if let Some((m, t)) = anim.action {
            let t = t + dt / m.duration();
            anim.action = (t < 1.0).then_some((m, t));
        }
        anim.hurt = anim.hurt.map(|t| t + dt).filter(|t| *t < 0.4);
        anim.dodge = anim.dodge.map(|t| t + dt).filter(|t| *t < 0.45);
        if let Some(d) = anim.dead.as_mut() {
            *d = (*d + dt / 1.1).min(1.0);
        }
        let pose = pose(rig, &anim, fun);
        for j in 0..JOINTS {
            if let Some(e) = rig.joints[j] {
                if let Ok(mut transform) = joints.get_mut(e) {
                    if j == J::Lift as usize {
                        // Lunges and falls are in the creature's own frame.
                        let facing = Quat::from_rotation_y(anim.yaw);
                        transform.translation = facing * pose.off[j] * rig.scale;
                        transform.rotation = facing * pose.rot[j];
                    } else {
                        transform.translation = rig.rest[j] + pose.off[j];
                        transform.rotation = pose.rot[j];
                    }
                }
            }
        }
        place_segments(rig, &anim, fun, &mut joints);
    }
}

fn pose(rig: &Rig, a: &Anim, fun: f32) -> Pose {
    let mut p = Pose::new();
    let s = a.speed;
    let w = a.walk;
    let breath = (a.time * 1.8 + a.seed).sin();
    let hurt = a
        .hurt
        .filter(|t| *t >= 0.0)
        .map_or(0.0, |t| (1.0 - t / 0.4) * (t / 0.06).min(1.0));
    let dodge = a
        .dodge
        .filter(|t| *t >= 0.0)
        .map_or(0.0, |t| (t / 0.45 * PI).sin());
    let (m, t) = a.action.map_or((None, 0.0), |(m, t)| (Some(m), t));
    // At rest each creature idles in its own way, fading out as it moves or acts.
    let calm = (1.0 - s).clamp(0.0, 1.0) * f32::from(u8::from(m.is_none() && a.dead.is_none()));
    if calm > 0.0 && !a.quiet {
        idle(&mut p, rig, a.time + a.seed * 3.7, a.seed, calm);
        if fun > 0.0 {
            let (w, gag, progress) = antic(a);
            if w > 0.0 {
                antics(&mut p, rig, a.time, gag, progress, w * calm * fun);
            }
        }
    }
    match rig.plan {
        Plan::Biped => biped_pose(&mut p, rig, s, w, breath, m, t),
        Plan::Bird => bird_pose(&mut p, s, w, breath, m, t),
        Plan::Quadruped | Plan::Centaur => quad_pose(&mut p, rig, s, w, breath, m, t),
        Plan::Serpent | Plan::Floater | Plan::Marker => {}
    }
    // Reactions, shared by every plan.
    p.r(J::Lift, Quat::from_rotation_x(-0.28 * hurt));
    p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * hurt));
    p.r(J::Head, bow(-0.35 * hurt));
    p.r(J::Neck, bow(-0.3 * hurt));
    let side = if a.seed > 5.0 { 1.0 } else { -1.0 };
    p.o(J::Lift, Vec3::new(side * 0.28 * dodge, 0.0, -0.05 * dodge));
    p.r(J::Lift, Quat::from_rotation_z(-side * 0.25 * dodge));
    if let Some(d) = a.dead {
        death_pose(&mut p, rig.plan, ease(d), a.seed);
    }
    p
}

/// Seconds without the player's touch before the cast starts to play.
pub const BORED_AFTER: f32 = 60.0;

/// Seconds since the player last touched keyboard or mouse.
#[derive(Resource, Default)]
pub struct Boredom(pub f32);

/// Where a creature is in its round of antics: how far into one (a weight
/// that ramps in and out), which gag, and its progress from 0 to 1.
fn antic(a: &Anim) -> (f32, u32, f32) {
    const PERIOD: f32 = 12.0;
    const START: f32 = 1.0;
    const END: f32 = 8.0;
    let local = a.time + a.seed * 3.7;
    let t = local.rem_euclid(PERIOD);
    let round = (local / PERIOD).floor() as i64;
    let gag = (round + a.seed as i64).rem_euclid(3) as u32;
    let w = ease((t - START) / 0.7) * ease((END - t) / 0.7);
    (w, gag, ((t - START) / (END - START)).clamp(0.0, 1.0))
}

/// A whole number of turns over a gag's progress, so spins start and end square.
fn spins(progress: f32, turns: f32) -> f32 {
    ease(progress) * TAU * turns
}

/// Little bits of fun while the player is away. `k` fades the gag in and
/// out; `progress` runs 0 to 1 over it, for spins and laps that must close.
fn antics(p: &mut Pose, rig: &Rig, time: f32, gag: u32, progress: f32, k: f32) {
    let beat = time * TAU * 1.6;
    match rig.plan {
        Plan::Biped => match (rig.style, gag % 2) {
            (Style::Caster, 0) => {
                // Twirls the staff like a baton.
                p.r(J::ArmR, fwd(0.9 * k) * Quat::from_rotation_z(-0.3 * k));
                p.r(J::ForeR, fwd(0.8 * k));
                p.r(
                    J::HandR,
                    Quat::from_rotation_z(spins(progress, 6.0) * k.signum()),
                );
                p.r(J::Head, Quat::from_rotation_y(-0.3 * k) * bow(0.2 * k));
            }
            (Style::Caster, _) => {
                // Floats cross-legged in meditation.
                p.o(J::Lift, Vec3::Y * (0.28 + (time * 1.7).sin() * 0.04) * k);
                for (leg, shin, side) in [(J::LegL, J::ShinL, 1.0f32), (J::LegR, J::ShinR, -1.0)] {
                    p.r(leg, Quat::from_rotation_y(side * 0.7 * k) * fwd(1.5 * k));
                    p.r(shin, fwd(-2.3 * k));
                }
                for (arm, fore, side) in [(J::ArmL, J::ForeL, 1.0f32), (J::ArmR, J::ForeR, -1.0)] {
                    p.r(arm, Quat::from_rotation_z(side * 0.35 * k) * fwd(0.4 * k));
                    p.r(fore, fwd(1.0 * k));
                }
                p.r(J::Head, bow(0.15 * k));
            }
            (Style::Sentinel, 0) => {
                // Marches on the spot and salutes.
                let step = beat.sin();
                p.r(J::LegL, fwd(1.1 * step.max(0.0) * k));
                p.r(J::ShinL, fwd(-1.3 * step.max(0.0) * k));
                p.r(J::LegR, fwd(1.1 * (-step).max(0.0) * k));
                p.r(J::ShinR, fwd(-1.3 * (-step).max(0.0) * k));
                p.o(J::Lift, Vec3::Y * 0.03 * step.abs() * k);
                p.r(J::ArmR, Quat::from_rotation_z(-1.3 * k) * fwd(0.5 * k));
                p.r(J::ForeR, fwd(2.0 * k));
                p.r(J::Head, bow(-0.1 * k));
            }
            (Style::Sentinel, _) => {
                // Polishes the shield with a gauntlet.
                p.r(J::ArmL, fwd(0.9 * k) * Quat::from_rotation_z(-0.3 * k));
                p.r(J::ForeL, fwd(0.9 * k));
                let rub = time * 9.0;
                p.r(
                    J::ArmR,
                    fwd((0.9 + 0.2 * rub.cos()) * k)
                        * Quat::from_rotation_z((0.25 + 0.25 * rub.sin()) * k),
                );
                p.r(J::ForeR, fwd(1.1 * k));
                p.r(J::Head, bow(0.35 * k) * Quat::from_rotation_y(0.3 * k));
            }
            (Style::Scout, 0) => {
                // Juggles the dagger.
                let toss = (time * 9.0).sin();
                p.r(J::ArmR, fwd(0.7 * k));
                p.r(J::ForeR, fwd((1.0 + 0.6 * toss) * k));
                p.r(
                    J::HandR,
                    Quat::from_rotation_x(spins(progress, 9.0) * k.signum()),
                );
                p.r(J::ArmL, fwd(0.6 * k));
                p.r(J::ForeL, fwd((1.0 - 0.6 * toss) * k));
                p.r(J::Head, bow(-0.35 * k + 0.15 * toss * k));
            }
            (Style::Scout, _) => {
                // Tiptoes in place, looking about with exaggerated stealth.
                let step = (time * 2.6).sin();
                p.o(J::Lift, Vec3::Y * -0.08 * k);
                p.r(J::LegL, fwd(0.9 * step.max(0.0) * k));
                p.r(J::ShinL, fwd(-1.4 * step.max(0.0) * k));
                p.r(J::LegR, fwd(0.9 * (-step).max(0.0) * k));
                p.r(J::ShinR, fwd(-1.4 * (-step).max(0.0) * k));
                p.r(J::Torso, bow(0.35 * k));
                for (arm, fore) in [(J::ArmL, J::ForeL), (J::ArmR, J::ForeR)] {
                    p.r(arm, fwd(0.7 * k));
                    p.r(fore, fwd(1.3 * k));
                }
                p.r(J::Head, Quat::from_rotation_y((time * 1.3).sin() * 0.8 * k));
            }
            (Style::Guardian, _) => {
                // The robot.
                let snap = |x: f32| x.sin().round();
                p.r(J::ArmL, fwd((0.8 + 0.7 * snap(time * 2.0)) * k));
                p.r(
                    J::ForeL,
                    fwd(1.57 * (0.5 + 0.5 * snap(time * 2.0 + 1.0)) * k),
                );
                p.r(J::ArmR, fwd((0.8 - 0.7 * snap(time * 2.0)) * k));
                p.r(
                    J::ForeR,
                    fwd(1.57 * (0.5 - 0.5 * snap(time * 2.0 + 1.0)) * k),
                );
                p.r(J::Head, Quat::from_rotation_y(0.8 * snap(time * 1.5) * k));
                p.r(J::Lift, Quat::from_rotation_y(0.35 * snap(time) * k));
            }
            (Style::Hobgoblin, 1) => {
                // Scratches its head with the club.
                p.r(J::ArmR, Quat::from_rotation_z(-1.7 * k) * fwd(0.5 * k));
                p.r(J::ForeR, fwd((2.1 + (time * 14.0).sin() * 0.15) * k));
                p.r(J::Head, Quat::from_rotation_z(0.3 * k));
                p.r(J::LegL, fwd(0.3 * k) * Quat::from_rotation_z(0.25 * k));
            }
            (Style::Zombie, 0) => {
                // Its head drops off; it catches it and puts it back.
                let drop = ease(progress * 3.0) * ease((1.0 - progress) * 3.0) * k;
                p.o(J::Head, Vec3::new(0.0, -0.62, 0.36) * drop);
                p.r(
                    J::Head,
                    Quat::from_rotation_x(1.2 * drop) * Quat::from_rotation_z(0.6 * drop),
                );
                for arm in [J::ArmL, J::ArmR] {
                    p.r(arm, fwd(1.0 * k));
                }
                p.r(J::ForeL, fwd(0.5 * k));
                p.r(J::ForeR, fwd(0.5 * k));
            }
            (Style::Zombie, _) => {
                // A shuffling graveyard dance, claws up.
                let side = (time * 2.2).sin();
                p.o(J::Lift, Vec3::X * 0.12 * side * k);
                p.r(J::Lift, Quat::from_rotation_z(-0.12 * side * k));
                p.r(J::ArmL, Quat::from_rotation_z(1.0 * k) * fwd(0.6 * k));
                p.r(J::ArmR, Quat::from_rotation_z(-0.4 * k) * fwd(1.2 * k));
                p.r(J::ForeL, fwd(1.2 * k));
                p.r(J::ForeR, fwd(0.9 * k));
                p.r(J::Head, Quat::from_rotation_z(0.35 * side * k));
            }
            (Style::Vampire, 0) => {
                // A swirling pirouette, cloak flared.
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 3.0)));
                p.o(J::Lift, Vec3::Y * 0.06 * k);
                p.r(J::ArmL, Quat::from_rotation_z(1.3 * k));
                p.r(J::ArmR, Quat::from_rotation_z(-1.3 * k));
            }
            (Style::Vampire, _) => {
                // Hides behind the cloak and rises, very dramatically.
                p.o(J::Lift, Vec3::Y * 0.18 * k);
                p.r(J::ArmL, fwd(1.4 * k) * Quat::from_rotation_z(-0.7 * k));
                p.r(J::ForeL, fwd(1.5 * k));
                p.r(J::Head, bow(0.25 * k) * Quat::from_rotation_y(-0.3 * k));
            }
            (_, 0) => jig(p, time, k),
            _ => stretch(p, time, k),
        },
        Plan::Bird => match gag {
            0 => zoomies(p, progress, 0.55, k),
            _ => {
                // Head-bob dance with side steps.
                let b = beat.sin();
                p.r(J::Neck, bow((0.5 + 0.5 * b) * k));
                p.o(
                    J::Lift,
                    Vec3::new(0.08 * (beat * 0.5).sin(), 0.04 * b.abs(), 0.0) * k,
                );
                p.r(J::LegL, fwd(0.4 * b.max(0.0) * k));
                p.r(J::LegR, fwd(0.4 * (-b).max(0.0) * k));
            }
        },
        Plan::Quadruped => match (rig.style, gag) {
            (_, 1) => {
                // Bucks, kicking out behind.
                let kick = (time * 5.0).sin().max(0.0) * k;
                p.r(J::Lift, Quat::from_rotation_x(0.3 * kick));
                p.o(J::Lift, Vec3::Y * 0.05 * kick);
                p.r(J::BL, fwd(-kick));
                p.r(J::BR, fwd(-1.1 * kick));
                p.r(J::Neck, bow(0.6 * kick));
            }
            (Style::Unicorn, _) | (_, 0) => {
                // Rears up, pawing the air.
                p.r(J::Lift, Quat::from_rotation_x(-0.65 * k));
                p.o(J::Lift, Vec3::new(0.0, 0.18, -0.3) * k);
                let paw = (time * 8.0).sin();
                p.r(J::FL, fwd((1.1 + 0.4 * paw) * k));
                p.r(J::FLs, fwd(-1.5 * k));
                p.r(J::FR, fwd((1.1 - 0.4 * paw) * k));
                p.r(J::FRs, fwd(-1.5 * k));
                p.r(J::Neck, bow(-0.4 * k));
            }
            _ => zoomies(p, progress, 0.7, k),
        },
        Plan::Centaur => {
            // A hoof-stamping dance, arms high.
            let b = beat.sin();
            p.r(J::FL, fwd(0.8 * b.max(0.0) * k));
            p.r(J::FR, fwd(0.8 * (-b).max(0.0) * k));
            p.r(J::Torso, Quat::from_rotation_y(0.4 * b * k));
            p.r(J::ArmL, Quat::from_rotation_z(2.4 * k));
            p.r(J::ArmR, Quat::from_rotation_z(-2.4 * k));
            p.r(J::Head, Quat::from_rotation_z(0.2 * b * k));
        }
        Plan::Serpent => {
            if gag != 0 {
                // Chases its own tail.
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 3.0)));
            }
        }
        Plan::Marker => {}
        Plan::Floater => match gag {
            0 => {
                // Loop the loop.
                let a = spins(progress, 2.0);
                p.o(
                    J::Lift,
                    Vec3::new(0.0, (1.0 - a.cos()) * 0.45, a.sin() * 0.45),
                );
            }
            _ => {
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 5.0)));
                p.o(J::Lift, Vec3::Y * 0.4 * k);
            }
        },
    }
}

/// A jig: hops, kicks and waving arms.
fn jig(p: &mut Pose, time: f32, k: f32) {
    let b = (time * TAU * 1.6).sin();
    p.o(J::Lift, Vec3::Y * 0.07 * b.abs() * k);
    p.r(J::LegL, fwd(0.7 * b.max(0.0) * k));
    p.r(J::ShinL, fwd(-0.9 * b.max(0.0) * k));
    p.r(J::LegR, fwd(0.7 * (-b).max(0.0) * k));
    p.r(J::ShinR, fwd(-0.9 * (-b).max(0.0) * k));
    p.r(J::ArmL, Quat::from_rotation_z((1.6 + 0.5 * b) * k));
    p.r(J::ArmR, Quat::from_rotation_z(-(1.6 - 0.5 * b) * k));
    p.r(J::ForeL, fwd(0.6 * k));
    p.r(J::ForeR, fwd(0.6 * k));
    p.r(J::Torso, Quat::from_rotation_z(0.12 * b * k));
    p.r(J::Head, Quat::from_rotation_z(-0.2 * b * k));
}

/// A long yawn and a stretch, arms over the head.
fn stretch(p: &mut Pose, time: f32, k: f32) {
    let wave = (time * 1.5).sin() * 0.1;
    p.r(J::ArmL, Quat::from_rotation_z((2.7 + wave) * k));
    p.r(J::ArmR, Quat::from_rotation_z(-(2.7 - wave) * k));
    p.r(J::ForeL, Quat::from_rotation_z(0.3 * k));
    p.r(J::ForeR, Quat::from_rotation_z(-0.3 * k));
    p.r(J::Torso, bow(-0.25 * k) * Quat::from_rotation_z(wave * k));
    p.r(J::Head, bow(-0.45 * k));
    p.o(J::Lift, Vec3::Y * 0.03 * k);
}

/// Racing round in a little circle, legs a blur.
fn zoomies(p: &mut Pose, progress: f32, r: f32, k: f32) {
    let a = spins(progress, 2.0);
    p.o(J::Lift, Vec3::new((1.0 - a.cos()) * r, 0.0, a.sin() * r));
    p.r(J::Lift, Quat::from_rotation_y(a));
    let run = progress * 120.0;
    for (j, phase) in [
        (J::LegL, 0.0),
        (J::LegR, PI),
        (J::FL, 0.0),
        (J::FR, PI),
        (J::BL, PI),
        (J::BR, 0.0),
    ] {
        p.r(j, fwd((run + phase).sin() * 0.8 * k));
    }
    p.r(J::Neck, bow(0.4 * k));
}

/// A bell over part of a repeating cycle: 0 outside [start, end] of each
/// `period` seconds, rising smoothly to 1 in the middle.
fn every(time: f32, period: f32, start: f32, end: f32) -> f32 {
    let t = time.rem_euclid(period);
    if t < start || t > end {
        0.0
    } else {
        ((t - start) / (end - start) * PI).sin().powi(2)
    }
}

/// Idle behaviour: breathing, weight shifts and looking about, with a
/// signature fidget every few seconds that fits the creature.
fn idle(p: &mut Pose, rig: &Rig, time: f32, seed: f32, k: f32) {
    let look = (time * 0.37).sin() * 0.3 + (time * 0.13).sin() * 0.2;
    let sway = (time * 0.6).sin();
    match rig.plan {
        Plan::Biped => {
            p.r(J::Lift, Quat::from_rotation_z(sway * 0.025 * k));
            p.r(J::Hips, Quat::from_rotation_z(-sway * 0.04 * k));
            p.r(J::LegL, Quat::from_rotation_z(sway * 0.03 * k));
            p.r(J::LegR, Quat::from_rotation_z(sway * 0.03 * k));
            p.r(J::Head, Quat::from_rotation_y(look * k));
            match rig.style {
                Style::Hero => {
                    // Lifts the blade to look along its edge.
                    let f = every(time, 8.0, 4.5, 7.0) * k;
                    p.r(J::ArmR, fwd(0.9 * f) * Quat::from_rotation_z(0.25 * f));
                    p.r(J::ForeR, fwd(0.9 * f));
                    p.r(J::HandR, fwd(0.9 * f) * Quat::from_rotation_z(0.6 * f));
                    p.r(J::Head, Quat::from_rotation_y(-0.35 * f) * bow(0.15 * f));
                }
                Style::Caster => {
                    // Raises a palm to the orb, which it studies.
                    let f = every(time, 7.0, 3.5, 6.3) * k;
                    p.r(J::ArmL, fwd(1.1 * f) * Quat::from_rotation_z(0.2 * f));
                    p.r(J::ForeL, fwd(0.8 * f + (time * 5.0).sin() * 0.08 * f));
                    p.r(J::ArmR, fwd(0.25 * f));
                    p.r(J::Head, Quat::from_rotation_y(0.25 * f) * bow(-0.1 * f));
                }
                Style::Sentinel => {
                    // Settles the shield and rolls a shoulder.
                    let f = every(time, 6.5, 3.8, 5.8) * k;
                    p.r(J::ArmL, fwd(0.55 * f));
                    p.r(J::ForeL, fwd(0.7 * f));
                    p.r(J::Torso, Quat::from_rotation_z(0.06 * f));
                }
                Style::Scout => {
                    // Light on its feet, glancing sharply about; flips the dagger.
                    let dart = ((time * 0.9).sin() * 3.0).clamp(-1.0, 1.0) * 0.45;
                    p.r(J::Head, Quat::from_rotation_y(dart * k));
                    p.o(J::Lift, Vec3::Y * -0.03 * k);
                    let f = every(time, 5.5, 3.0, 4.2) * k;
                    p.r(J::HandR, Quat::from_rotation_y(f * TAU));
                    p.r(J::ArmR, fwd(0.4 * f));
                }
                Style::Guardian => {
                    // A slow, grinding scan of the room.
                    p.r(
                        J::Head,
                        Quat::from_rotation_y((time * 0.22).sin() * 0.6 * k),
                    );
                    p.r(
                        J::Torso,
                        Quat::from_rotation_y((time * 0.22).sin() * 0.12 * k),
                    );
                }
                Style::Hobgoblin => {
                    // Twitchy, and taps the club on its shoulder.
                    let twitch = ((time * 4.3).sin() * (time * 1.7).sin()).max(0.0) * 0.25;
                    p.r(J::Head, Quat::from_rotation_z(twitch * k));
                    let f = every(time, 6.0, 2.5, 5.0) * k;
                    let tap = (time * 9.0).sin().max(0.0) * 0.15;
                    p.r(J::ArmR, fwd(-0.4 * f) * Quat::from_rotation_z(-0.3 * f));
                    p.r(J::ForeR, fwd((1.9 + tap) * f));
                }
                Style::Zombie => {
                    // Sways and lolls.
                    p.r(
                        J::Lift,
                        Quat::from_rotation_z((time * 0.8).sin() * 0.08 * k),
                    );
                    p.r(
                        J::Head,
                        Quat::from_rotation_z((time * 0.55).sin() * 0.35 * k) * bow(0.15 * k),
                    );
                    p.r(J::ArmL, fwd((time * 0.7).sin() * 0.15 * k));
                    p.r(J::ArmR, fwd((time * 0.7 + 1.5).sin() * 0.15 * k));
                }
                Style::Vampire => {
                    // Still and watchful, with a slow tilt of the head.
                    p.r(
                        J::Head,
                        Quat::from_rotation_z((time * 0.3).sin() * 0.12 * k),
                    );
                    p.o(J::Lift, Vec3::Y * ((time * 0.9).sin() * 0.012 * k));
                    let f = every(time, 9.0, 5.0, 8.0) * k;
                    p.r(J::ArmL, fwd(0.9 * f) * Quat::from_rotation_z(-0.5 * f));
                    p.r(J::ForeL, fwd(1.3 * f));
                }
                Style::Yeti => {
                    // Heavy breaths, and now and then beats its chest.
                    p.r(J::Torso, bow((time * 1.2).sin() * 0.05 * k));
                    let f = every(time, 9.0, 5.5, 7.5) * k;
                    let beat = (time * 12.0).sin();
                    p.r(J::ArmL, fwd((0.9 + 0.25 * beat) * f));
                    p.r(J::ArmR, fwd((0.9 - 0.25 * beat) * f));
                    p.r(J::ForeL, fwd(1.2 * f));
                    p.r(J::ForeR, fwd(1.2 * f));
                    p.r(J::Head, bow(-0.3 * f));
                }
                _ => {}
            }
        }
        Plan::Bird => {
            // Head bobs and jerks; every few seconds it pecks at the ground.
            let jerk = ((time * 1.3).sin() * 4.0).round() * 0.08;
            p.r(J::Neck, Quat::from_rotation_y(jerk * k));
            let f = every(time, 5.0, 2.8, 3.8) * k;
            let peck = (time * 14.0).sin().max(0.0);
            p.r(J::Neck, bow((1.6 + 0.2 * peck) * f));
            p.r(J::Head, bow(0.4 * f));
            p.r(J::Body, bow(0.25 * f));
        }
        Plan::Quadruped | Plan::Centaur => {
            p.r(
                J::Tail,
                Quat::from_rotation_y((time * 2.1).sin() * 0.35 * k),
            );
            p.r(J::Body, Quat::from_rotation_z(sway * 0.02 * k));
            if rig.plan == Plan::Centaur {
                p.r(J::Head, Quat::from_rotation_y(look * k));
                return;
            }
            // Grazes, then lifts its head; stamps a forefoot now and then.
            let graze = every(time, 10.0, 4.0, 8.5) * k;
            let chew = (time * 6.0).sin() * 0.05 * graze;
            p.r(
                J::Neck,
                bow((1.15 + chew) * graze) * Quat::from_rotation_y(look * 0.4 * (k - graze)),
            );
            p.r(J::Snout, bow(0.35 * graze));
            let stamp = every(time + seed, 7.0, 1.0, 1.6) * k;
            p.r(J::FL, fwd(0.5 * stamp));
            p.r(J::FLs, fwd(-0.9 * stamp));
        }
        Plan::Serpent | Plan::Floater | Plan::Marker => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn biped_pose(p: &mut Pose, rig: &Rig, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32) {
    let zombie = rig.style == Style::Zombie;
    let lumber = matches!(rig.style, Style::Guardian | Style::Yeti);
    let stride = if zombie { 0.38 } else { 0.55 };
    let swing = w.sin() * stride * s;
    p.r(J::LegL, fwd(swing));
    p.r(J::LegR, fwd(-swing * if zombie { 0.6 } else { 1.0 }));
    p.r(J::ShinL, fwd(-((w + 0.9).sin().max(0.0) * 0.9 * s)));
    p.r(J::ShinR, fwd(-((w + 0.9 + PI).sin().max(0.0) * 0.9 * s)));
    let bob = (w * 2.0).cos().abs() * 0.04 * s;
    p.o(J::Lift, Vec3::Y * (bob - 0.02 * s));
    p.r(
        J::Torso,
        Quat::from_rotation_y(w.sin() * 0.12 * s) * bow(rig.hunch + 0.08 * s + breath * 0.015),
    );
    if lumber {
        p.r(J::Lift, Quat::from_rotation_z(w.sin() * 0.06 * s));
    }
    if zombie {
        // Arms out ahead, swaying; a dragging gait.
        p.r(
            J::ArmL,
            fwd(1.25 + breath * 0.05) * Quat::from_rotation_z(0.1),
        );
        p.r(
            J::ArmR,
            fwd(1.15 - breath * 0.05) * Quat::from_rotation_z(-0.1),
        );
        p.r(J::Head, Quat::from_rotation_z(0.25 + breath * 0.05));
        p.r(J::Lift, Quat::from_rotation_z(w.sin() * 0.1 * s));
    } else {
        p.r(
            J::ArmL,
            fwd(-swing * 0.7) * Quat::from_rotation_z(0.1 + breath * 0.01),
        );
        p.r(
            J::ArmR,
            fwd(swing * 0.7) * Quat::from_rotation_z(-0.1 - breath * 0.01),
        );
        p.r(J::ForeL, fwd(0.25 + 0.2 * s));
        p.r(J::ForeR, fwd(0.3 + 0.2 * s));
    }
    p.r(J::Head, bow(-rig.hunch * 0.7 + breath * 0.02));
    // At rest the weapon hangs point-down from the fist; a move takes it over.
    let busy = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.85, 1.0));
    if !zombie {
        p.r(J::HandR, fwd(-0.8 * (1.0 - busy)));
    }
    let Some(m) = m else {
        return;
    };
    match m {
        Move::Strike => {
            let wind = envelope(t, 0.0, 0.38, 0.38, 0.4);
            let hit = envelope(t, 0.38, 0.5, 0.6, 1.0);
            p.r(
                J::ArmR,
                fwd(-2.1 * wind + 1.25 * hit) * Quat::from_rotation_z(-0.3 * wind),
            );
            p.r(J::ForeR, fwd(0.9 * wind + 0.1 * hit));
            p.r(J::HandR, fwd(-0.5 * wind + 1.2 * hit));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.4 * wind - 0.35 * hit) * bow(-0.1 * wind + 0.25 * hit),
            );
            p.o(J::Lift, Vec3::new(0.0, 0.0, -0.06 * wind + 0.3 * hit));
            p.r(J::LegR, fwd(-0.3 * hit));
            p.r(J::LegL, fwd(0.45 * hit));
        }
        Move::Heavy => {
            let up = envelope(t, 0.0, 0.45, 0.45, 0.48);
            let slam = envelope(t, 0.45, 0.56, 0.7, 1.0);
            for (arm, fore, hand) in [(J::ArmL, J::ForeL, J::HandL), (J::ArmR, J::ForeR, J::HandR)]
            {
                p.r(arm, fwd(-2.7 * up + 1.0 * slam));
                p.r(fore, fwd(0.5 * up));
                p.r(hand, fwd(-0.4 * up + 1.1 * slam));
            }
            p.r(J::ArmL, Quat::from_rotation_z(-0.3 * (up + slam)));
            p.r(J::ArmR, Quat::from_rotation_z(0.3 * (up + slam)));
            p.r(J::Torso, bow(-0.25 * up + 0.55 * slam));
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.05 * up - 0.18 * slam, 0.25 * slam),
            );
            p.r(J::LegL, fwd(0.5 * slam));
            p.r(J::ShinL, fwd(-0.6 * slam));
            p.r(J::ShinR, fwd(-0.4 * slam));
        }
        Move::Cast(_) => {
            let gather = envelope(t, 0.0, 0.4, 0.42, 0.5);
            let release = envelope(t, 0.42, 0.5, 0.65, 1.0);
            p.r(
                J::ArmR,
                fwd(0.9 * gather + 1.5 * release) * Quat::from_rotation_z(-0.2 * gather),
            );
            p.r(
                J::ArmL,
                fwd(1.1 * gather + 0.5 * release) * Quat::from_rotation_z(0.4 * gather),
            );
            p.r(J::ForeL, fwd(1.0 * gather));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.25 * gather - 0.2 * release)
                    * bow(-0.12 * gather + 0.18 * release),
            );
            p.r(J::Head, bow(-0.15 * gather));
            p.o(J::Lift, Vec3::new(0.0, 0.04 * gather, 0.12 * release));
        }
        Move::Hex => {
            let k = envelope(t, 0.0, 0.35, 0.7, 1.0);
            p.r(J::ArmL, fwd(1.3 * k) * Quat::from_rotation_z(0.6 * k));
            p.r(J::ArmR, fwd(1.3 * k) * Quat::from_rotation_z(-0.6 * k));
            p.r(J::ForeL, fwd(-0.2 * k));
            p.r(J::ForeR, fwd(-0.2 * k));
            p.r(
                J::Head,
                bow(0.2 * k) * Quat::from_rotation_z((t * 18.0).sin() * 0.04 * k),
            );
            p.o(J::Lift, Vec3::Y * 0.06 * k);
        }
        Move::Guard(_) => {
            let k = envelope(t, 0.0, 0.2, 0.75, 1.0);
            p.o(J::Lift, Vec3::new(0.0, -0.13 * k, -0.04 * k));
            p.r(J::ArmL, fwd(1.25 * k) * Quat::from_rotation_z(-0.55 * k));
            p.r(J::ForeL, fwd(1.0 * k));
            p.r(J::ArmR, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.6 * k));
            p.r(J::Torso, bow(0.2 * k));
            p.r(J::LegL, fwd(0.45 * k));
            p.r(J::ShinL, fwd(-0.7 * k));
            p.r(J::LegR, fwd(-0.25 * k));
            p.r(J::ShinR, fwd(-0.5 * k));
        }
        Move::Thrust => {
            let draw = envelope(t, 0.0, 0.3, 0.3, 0.34);
            let lunge = envelope(t, 0.32, 0.4, 0.55, 1.0);
            p.r(
                J::ArmR,
                fwd(-0.5 * draw + 1.45 * lunge) * Quat::from_rotation_z(-0.15 * draw),
            );
            p.r(J::ForeR, fwd(1.3 * draw));
            p.r(J::HandR, fwd(0.2 * draw + 1.45 * lunge));
            p.r(
                J::ArmL,
                fwd(-0.6 * lunge) * Quat::from_rotation_z(0.5 * lunge),
            );
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.35 * draw - 0.45 * lunge) * bow(0.15 * lunge),
            );
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.08 * lunge, -0.1 * draw + 0.45 * lunge),
            );
            p.r(J::LegL, fwd(0.75 * lunge));
            p.r(J::ShinL, fwd(-0.5 * lunge));
            p.r(J::LegR, fwd(-0.45 * lunge));
        }
        Move::Combo(other) => {
            // A backhand: the arm crosses the body, then sweeps out.
            let (arm, fore, hand, sign) = if other {
                (J::ArmL, J::ForeL, J::HandL, -1.0)
            } else {
                (J::ArmR, J::ForeR, J::HandR, 1.0)
            };
            let cross = envelope(t, 0.0, 0.3, 0.3, 0.33);
            let sweep = envelope(t, 0.32, 0.42, 0.55, 1.0);
            p.r(
                arm,
                fwd(0.9 * cross + 1.1 * sweep)
                    * Quat::from_rotation_z(sign * (1.1 * cross - 0.9 * sweep)),
            );
            p.r(fore, fwd(1.1 * cross + 0.2 * sweep));
            p.r(hand, fwd(0.6 * sweep));
            p.r(
                J::Torso,
                Quat::from_rotation_y(sign * (-0.5 * cross + 0.55 * sweep)),
            );
            p.o(J::Lift, Vec3::new(0.0, -0.04 * sweep, 0.2 * sweep));
            p.r(J::LegL, fwd(0.3 * sweep));
        }
        Move::Hook => {
            let low = envelope(t, 0.0, 0.35, 0.4, 0.8);
            let rip = envelope(t, 0.38, 0.48, 0.55, 1.0);
            p.o(J::Lift, Vec3::new(0.0, -0.18 * low, 0.25 * rip));
            p.r(
                J::ArmR,
                fwd(0.3 * low + 0.9 * rip) * Quat::from_rotation_z(-0.9 * low + 1.1 * rip),
            );
            p.r(J::ForeR, fwd(0.4 * low + 0.8 * rip));
            p.r(J::HandR, fwd(0.9 * low) * Quat::from_rotation_y(1.2 * rip));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.5 * low - 0.6 * rip) * bow(0.35 * low),
            );
            p.r(J::LegL, fwd(0.6 * low));
            p.r(J::ShinL, fwd(-0.9 * low));
            p.r(J::ShinR, fwd(-0.6 * low));
        }
        Move::Bash => {
            let set = envelope(t, 0.0, 0.3, 0.3, 0.33);
            let punch = envelope(t, 0.32, 0.42, 0.55, 1.0);
            p.r(
                J::ArmL,
                fwd(0.8 * set + 1.45 * punch) * Quat::from_rotation_z(-0.45 * set - 0.2 * punch),
            );
            p.r(J::ForeL, fwd(1.0 * set - 0.1 * punch));
            p.r(J::ArmR, fwd(-0.6 * set));
            p.r(
                J::Torso,
                Quat::from_rotation_y(-0.3 * set + 0.3 * punch) * bow(0.25 * punch),
            );
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.06 * set, -0.08 * set + 0.42 * punch),
            );
            p.r(J::LegL, fwd(0.6 * punch));
            p.r(J::LegR, fwd(-0.4 * punch));
        }
        Move::Shove => {
            let load = envelope(t, 0.0, 0.32, 0.32, 0.35);
            let drive = envelope(t, 0.34, 0.45, 0.6, 1.0);
            for (arm, fore) in [(J::ArmL, J::ForeL), (J::ArmR, J::ForeR)] {
                p.r(arm, fwd(0.7 * load + 1.5 * drive));
                p.r(fore, fwd(1.3 * load - 0.2 * drive));
            }
            p.r(J::ArmL, Quat::from_rotation_z(-0.2 * drive));
            p.r(J::ArmR, Quat::from_rotation_z(0.2 * drive));
            p.r(J::Torso, bow(0.2 * load + 0.45 * drive));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.12 * load - 0.05 * drive, -0.12 * load + 0.5 * drive),
            );
            p.r(J::LegL, fwd(0.5 * load + 0.8 * drive));
            p.r(J::ShinL, fwd(-0.7 * load - 0.3 * drive));
            p.r(J::LegR, fwd(-0.6 * drive));
        }
        Move::Gesture => {
            let k = envelope(t, 0.0, 0.25, 0.7, 1.0);
            p.r(J::ArmR, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.4 * k));
            p.r(J::Head, bow(0.1 * k));
        }
        Move::Focus => {
            let k = envelope(t, 0.0, 0.25, 0.8, 1.0);
            p.r(J::ArmR, fwd(1.2 * k) * Quat::from_rotation_z(0.35 * k));
            p.r(J::ForeR, fwd(1.0 * k));
            p.r(J::HandR, fwd(-0.7 * k));
            p.r(J::ArmL, fwd(0.5 * k) * Quat::from_rotation_z(0.3 * k));
            p.r(
                J::Head,
                Quat::from_rotation_z(0.18 * k * (t * 6.0).sin()) * bow(0.1 * k),
            );
            p.r(J::Torso, Quat::from_rotation_y(0.25 * k));
            p.o(J::Lift, Vec3::new(0.0, -0.08 * k, 0.0));
            p.r(J::LegL, fwd(0.35 * k));
            p.r(J::LegR, fwd(-0.3 * k));
            p.r(J::ShinR, fwd(-0.3 * k));
        }
        Move::Fear => {
            let draw = envelope(t, 0.0, 0.3, 0.3, 0.35);
            let cry = envelope(t, 0.33, 0.42, 0.75, 1.0);
            p.r(
                J::ArmL,
                fwd(-0.4 * cry - 0.2 * draw) * Quat::from_rotation_z(1.3 * cry + 0.3 * draw),
            );
            p.r(
                J::ArmR,
                fwd(-0.4 * cry - 0.2 * draw) * Quat::from_rotation_z(-1.3 * cry - 0.3 * draw),
            );
            p.r(J::ForeL, fwd(0.6 * cry));
            p.r(J::ForeR, fwd(0.6 * cry));
            p.r(J::Torso, bow(0.35 * draw - 0.3 * cry));
            p.r(
                J::Head,
                bow(0.3 * draw + 0.35 * cry) * Quat::from_rotation_z((t * 40.0).sin() * 0.05 * cry),
            );
            p.o(J::Lift, Vec3::new(0.0, 0.06 * cry, 0.12 * cry));
        }
        Move::Drain => {
            let reach = envelope(t, 0.0, 0.3, 0.45, 0.6);
            let draw = envelope(t, 0.4, 0.6, 0.85, 1.0);
            p.r(
                J::ArmR,
                fwd(1.5 * reach + 0.9 * draw) * Quat::from_rotation_z(-0.1 * reach),
            );
            p.r(J::ForeR, fwd(-0.1 * reach + 1.6 * draw));
            p.r(
                J::ArmL,
                fwd(0.9 * reach) * Quat::from_rotation_z(0.4 * reach),
            );
            p.r(J::Torso, bow(0.25 * reach - 0.2 * draw));
            p.r(J::Head, bow(0.15 * reach - 0.45 * draw));
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.05 * draw, 0.25 * reach - 0.1 * draw),
            );
        }
        Move::Touch(_) => {
            let coil = envelope(t, 0.0, 0.3, 0.3, 0.35);
            let reach = envelope(t, 0.33, 0.45, 0.6, 1.0);
            p.r(J::ArmR, fwd(0.3 * coil + 1.45 * reach));
            p.r(J::ForeR, fwd(1.2 * coil));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.3 * coil - 0.25 * reach) * bow(0.2 * reach),
            );
            p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * coil + 0.35 * reach));
            p.r(J::LegL, fwd(0.45 * reach));
        }
        Move::Step => {
            let back = envelope(t, 0.0, 0.35, 0.7, 1.0);
            let lift = (t / 0.35 * PI).sin().max(0.0);
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.05 * lift - 0.08 * back, -0.4 * back),
            );
            p.r(J::LegR, fwd(-0.6 * lift));
            p.r(J::ShinR, fwd(-0.8 * lift));
            p.r(
                J::ArmL,
                fwd(1.1 * back) * Quat::from_rotation_z(-0.5 * back),
            );
            p.r(J::ForeL, fwd(0.9 * back));
            p.r(J::ArmR, fwd(0.7 * back));
            p.r(J::ForeR, fwd(0.6 * back));
            p.r(J::Torso, bow(0.15 * back));
        }
        Move::Mend => {
            // Down on one knee, hands working a bandage round the arm.
            let kneel = envelope(t, 0.0, 0.25, 0.8, 1.0);
            let wrap = (t * 14.0).sin() * kneel;
            p.o(J::Lift, Vec3::new(0.0, -0.4 * kneel, 0.0));
            p.r(J::LegL, fwd(1.35 * kneel));
            p.r(J::ShinL, fwd(-1.45 * kneel));
            p.r(J::LegR, fwd(-0.15 * kneel));
            p.r(J::ShinR, fwd(-1.5 * kneel));
            p.r(
                J::ArmL,
                fwd(0.9 * kneel) * Quat::from_rotation_z(-0.35 * kneel),
            );
            p.r(J::ForeL, fwd(1.1 * kneel));
            p.r(
                J::ArmR,
                fwd((0.9 + 0.15 * wrap) * kneel)
                    * Quat::from_rotation_z((0.5 + 0.2 * wrap) * kneel),
            );
            p.r(J::ForeR, fwd(1.3 * kneel));
            p.r(J::Torso, bow(0.3 * kneel));
            p.r(J::Head, bow(0.45 * kneel));
        }
    }
}

fn bird_pose(p: &mut Pose, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32) {
    let swing = w.sin() * 0.6 * s;
    p.r(J::LegL, fwd(swing));
    p.r(J::LegR, fwd(-swing));
    p.r(J::ShinL, fwd(((w + 1.0).sin().max(0.0)) * 0.9 * s));
    p.r(J::ShinR, fwd(((w + 1.0 + PI).sin().max(0.0)) * 0.9 * s));
    p.o(J::Lift, Vec3::Y * ((w * 2.0).cos().abs() * 0.05 * s));
    p.r(
        J::Body,
        Quat::from_rotation_z(w.sin() * 0.06 * s) * bow(0.1 * s + breath * 0.02),
    );
    // The head pumps with each step and twitches when still.
    let twitch = ((breath * 3.0).sin() * 3.0).round() * 0.06 * (1.0 - s);
    p.r(
        J::Neck,
        bow((w * 2.0).sin() * 0.18 * s + 0.1 * s) * Quat::from_rotation_y(twitch),
    );
    p.r(J::Head, bow(-(w * 2.0).sin() * 0.15 * s));
    if let Some(m) = m {
        let wind = envelope(t, 0.0, 0.35, 0.35, 0.38);
        let peck = envelope(t, 0.38, 0.47, 0.55, 1.0);
        match m {
            m if m.offensive() => {
                p.r(J::Neck, bow(-0.7 * wind + 1.5 * peck));
                p.r(J::Head, bow(0.3 * wind + 0.4 * peck));
                p.r(J::Body, bow(-0.2 * wind + 0.35 * peck));
                p.o(J::Lift, Vec3::new(0.0, 0.0, 0.3 * peck));
                if matches!(m, Move::Heavy | Move::Shove | Move::Hook) {
                    p.r(J::LegR, fwd(1.3 * peck));
                }
            }
            _ => {
                let k = envelope(t, 0.0, 0.3, 0.7, 1.0);
                p.r(J::Neck, bow(-0.5 * k));
                p.r(J::Body, bow(-0.25 * k));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn quad_pose(p: &mut Pose, rig: &Rig, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32) {
    let a = w.sin() * 0.45 * s;
    let b = (w + PI).sin() * 0.45 * s;
    p.r(J::FL, fwd(a));
    p.r(J::BR, fwd(a));
    p.r(J::FR, fwd(b));
    p.r(J::BL, fwd(b));
    p.r(J::FLs, fwd(-((w + 0.8).sin().max(0.0)) * 0.8 * s));
    p.r(J::FRs, fwd(-((w + 0.8 + PI).sin().max(0.0)) * 0.8 * s));
    p.r(J::BLs, fwd(((w + 0.8 + PI).sin().max(0.0)) * 0.6 * s));
    p.r(J::BRs, fwd(((w + 0.8).sin().max(0.0)) * 0.6 * s));
    p.o(J::Lift, Vec3::Y * ((w * 2.0).cos().abs() * 0.04 * s));
    p.r(J::Body, bow(breath * 0.01));
    p.r(J::Neck, bow((w * 2.0).sin() * 0.08 * s + breath * 0.03));
    p.r(J::Snout, bow(breath * 0.04));
    p.r(
        J::Tail,
        Quat::from_rotation_y((w * 1.5).sin() * 0.3 * s + breath * 0.15),
    );
    // A centaur's rider swings arms like a walker.
    p.r(J::ArmL, fwd(-a * 0.6) * Quat::from_rotation_z(0.1));
    p.r(J::ArmR, fwd(0.4) * Quat::from_rotation_z(-0.1));
    p.r(J::ForeR, fwd(0.8));
    let Some(m) = m else {
        return;
    };
    if !m.offensive() {
        let k = envelope(t, 0.0, 0.25, 0.75, 1.0);
        p.r(J::Body, bow(-0.15 * k));
        p.o(
            J::Lift,
            Vec3::new(
                0.0,
                -0.06 * k,
                -0.25 * k * f32::from(u8::from(m == Move::Step)),
            ),
        );
        p.r(J::Neck, bow(-0.4 * k));
        p.r(J::ArmL, fwd(1.0 * k));
        return;
    }
    if rig.plan == Plan::Centaur {
        let wind = envelope(t, 0.0, 0.38, 0.38, 0.4);
        let thrust = envelope(t, 0.4, 0.5, 0.6, 1.0);
        p.r(J::ArmR, fwd(-0.6 * wind + 1.0 * thrust));
        p.r(J::ForeR, fwd(-0.6 * thrust));
        p.r(
            J::Torso,
            Quat::from_rotation_y(0.4 * wind - 0.3 * thrust) * bow(0.3 * thrust),
        );
        p.o(J::Lift, Vec3::new(0.0, 0.0, 0.3 * thrust));
        return;
    }
    let rear = envelope(t, 0.0, 0.38, 0.4, 0.5);
    let fall = envelope(t, 0.42, 0.52, 0.6, 1.0);
    let gore = rig.style == Style::Unicorn;
    // Rear up on the hind legs, striking out, then come down hard.
    p.r(J::Body, bow(-0.55 * rear + 0.15 * fall));
    p.o(J::Body, Vec3::new(0.0, 0.25 * rear, -0.12 * rear));
    for (thigh, shin) in [(J::FL, J::FLs), (J::FR, J::FRs)] {
        p.r(thigh, fwd(1.0 * rear + (t * 30.0).sin() * 0.25 * rear));
        p.r(shin, fwd(-1.1 * rear));
    }
    p.r(
        J::Neck,
        bow(-0.35 * rear + if gore { 1.0 } else { 0.6 } * fall),
    );
    p.r(J::Snout, bow(if gore { 0.5 } else { 0.2 } * fall));
    p.o(J::Lift, Vec3::new(0.0, 0.0, 0.32 * fall));
}

fn death_pose(p: &mut Pose, plan: Plan, d: f32, seed: f32) {
    let side = if seed > 5.0 { 1.0 } else { -1.0 };
    // A little variety from the seed, so no two bodies lie quite alike.
    let v = |k: u32| rand(u64::from(seed.to_bits()), k) - 0.5;
    match plan {
        Plan::Biped => {
            // Crumpled onto the back: tipped over along the rig's x axis and
            // rolled face-up, flat to the floor.
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d)
                * Quat::from_rotation_y(side * FRAC_PI_2 * d)
                * Quat::from_rotation_x(0.08 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.86 * d, 0.1 * d, 0.0);
            // Arms flung loose to the floor, elbows soft.
            p.r(
                J::ArmL,
                Quat::from_rotation_z((0.85 + v(1)) * d) * fwd(-0.35 * d),
            );
            p.r(
                J::ArmR,
                Quat::from_rotation_z(-(0.55 + v(2)) * d) * fwd(-0.35 * d),
            );
            // Twisted so the elbows fold along the floor, not up off it.
            p.r(
                J::ForeL,
                Quat::from_rotation_y(-FRAC_PI_2 * d) * fwd((0.45 + v(3) * 0.5) * d),
            );
            p.r(
                J::ForeR,
                Quat::from_rotation_y(FRAC_PI_2 * d) * fwd((0.7 + v(4) * 0.5) * d),
            );
            p.r(J::HandL, fwd(-0.4 * d));
            p.r(J::HandR, fwd(-0.4 * d));
            // Legs splayed, one knee fallen outward.
            p.r(
                J::LegL,
                Quat::from_rotation_z(0.22 * d)
                    * Quat::from_rotation_y(0.5 * d)
                    * fwd((0.35 + v(5) * 0.3) * d),
            );
            p.r(J::ShinL, fwd(-0.55 * d));
            p.r(
                J::LegR,
                Quat::from_rotation_z(-0.12 * d) * Quat::from_rotation_y(-0.25 * d),
            );
            p.r(J::ShinR, fwd(-0.12 * d));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.15 * side * d) * bow(-0.08 * d),
            );
            // The head lolls to one side.
            p.r(
                J::Head,
                Quat::from_rotation_y(side * 1.0 * d) * bow(-0.3 * d),
            );
        }
        Plan::Centaur => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.72 * d, 0.2 * d, 0.0);
            limp_legs(p, side, d);
            // The rider's half slumps over onto the floor.
            p.r(
                J::Torso,
                Quat::from_rotation_z(side * 0.9 * d) * bow(0.4 * d),
            );
            p.r(J::ArmL, Quat::from_rotation_z(1.2 * d));
            p.r(J::ArmR, Quat::from_rotation_z(-0.3 * d) * fwd(0.6 * d));
            p.r(
                J::Head,
                Quat::from_rotation_y(side * 0.8 * d) * bow(0.4 * d),
            );
        }
        Plan::Quadruped => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.72 * d, 0.2 * d, 0.0);
            limp_legs(p, side, d);
            // Neck stretched out along the ground, head resting on it.
            p.r(
                J::Neck,
                bow(1.05 * d) * Quat::from_rotation_z(-side * 0.15 * d),
            );
            p.r(J::Snout, bow(-0.3 * d));
            p.r(
                J::Tail,
                fwd(0.6 * d) * Quat::from_rotation_z(-side * 0.6 * d),
            );
        }
        Plan::Bird => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * 1.5 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.62 * d, 0.2 * d, 0.0);
            // Long legs sprawled, the neck laid flat with the head on the floor.
            p.r(
                J::LegL,
                fwd(0.6 * d) * Quat::from_rotation_z(-side * 0.35 * d),
            );
            p.r(J::ShinL, fwd(-0.2 * d));
            p.r(
                J::LegR,
                fwd(-0.2 * d) * Quat::from_rotation_z(-side * 0.1 * d),
            );
            p.r(J::ShinR, fwd(-0.5 * d));
            p.r(
                J::Neck,
                bow(1.25 * d) * Quat::from_rotation_z(-side * 0.35 * d),
            );
            p.r(J::Head, bow(-0.4 * d));
        }
        Plan::Serpent | Plan::Floater | Plan::Marker => {}
    }
}

/// Four legs gone slack: the forelegs reach, the hind legs trail, and the
/// legs on top sag down toward the floor.
fn limp_legs(p: &mut Pose, side: f32, d: f32) {
    let sag = |left: bool| {
        if (side > 0.0) == left {
            -side * 0.35
        } else {
            -side * 0.08
        }
    };
    p.r(J::FL, Quat::from_rotation_z(sag(true) * d) * fwd(0.55 * d));
    p.r(J::FR, Quat::from_rotation_z(sag(false) * d) * fwd(0.3 * d));
    p.r(J::BL, Quat::from_rotation_z(sag(true) * d) * fwd(-0.35 * d));
    p.r(
        J::BR,
        Quat::from_rotation_z(sag(false) * d) * fwd(-0.55 * d),
    );
    for shin in [J::FLs, J::FRs, J::BLs, J::BRs] {
        p.r(shin, fwd(-0.18 * d));
    }
    p.r(J::Body, Quat::from_rotation_x(0.05 * d));
}

/// Snakes and floaters place their pieces along paths rather than joints.
fn place_segments(rig: &Rig, a: &Anim, fun: f32, joints: &mut Query<&mut Transform, Without<Rig>>) {
    let (m, t) = a.action.map_or((None, 0.0), |(m, t)| (Some(m), t));
    let dead = a.dead.map(ease);
    let calm =
        (1.0 - a.speed).clamp(0.0, 1.0) * f32::from(u8::from(m.is_none() && a.dead.is_none()));
    let (w, gag, _) = antic(a);
    let spring = if gag == 0 { w * fun * calm } else { 0.0 };
    match rig.plan {
        Plan::Serpent => {
            let n = rig.segments.len();
            let amp = 0.05 + 0.1 * a.speed;
            let wave = a.walk * 2.5 + a.time * 1.5;
            let coil = envelope(t, 0.0, 0.35, 0.38, 0.45) * f32::from(u8::from(m.is_some()));
            let lunge = envelope(t, 0.4, 0.5, 0.6, 1.0) * f32::from(u8::from(m.is_some()));
            let d = dead.unwrap_or(0.0);
            let mut prev: Option<Vec3> = None;
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                let z = 0.6 - f * 0.068 + 0.35 * lunge - 0.15 * coil;
                let x = (wave - f * 0.5).sin() * amp * (0.3 + f / n as f32) * (1.0 - d)
                    + d * (f * 0.4).sin() * 0.12;
                let front = (1.0 - f / 6.0).max(0.0);
                let rest = (1.0 - a.speed) * f32::from(u8::from(m.is_none()));
                let rear = every(a.time + a.seed * 3.7, 8.0, 3.0, 6.5) * rest;
                let y = 0.07
                    + front * (0.32 * coil + 0.18 * lunge + 0.22 * rear) * (1.0 - d)
                    + (a.time * 2.0 + f).sin() * 0.005;
                let mut at = Vec3::new(x, y, z);
                if spring > 0.0 {
                    // Coiled up like a spring, head on top, bouncing.
                    let turn = f * 0.9;
                    let bounce = (a.time * 7.0).sin().abs() * 0.12;
                    let coil = Vec3::new(
                        turn.cos() * 0.13,
                        0.45 - f * 0.022 + bounce * (1.0 - f / n as f32),
                        turn.sin() * 0.13,
                    );
                    at = at.lerp(coil, spring);
                }
                let dir = prev.map_or(Vec3::Z, |p: Vec3| (p - at).normalize_or(Vec3::Z));
                prev = Some(at);
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = at;
                    let face = if i == 0 { Vec3::Z } else { dir };
                    transform.rotation =
                        Quat::from_rotation_arc(Vec3::Z, face) * Quat::from_rotation_z(d * 2.6);
                }
            }
            // The tongue flickers out every couple of seconds, and on a strike.
            if let Some(tongue) = rig.joint(J::Snout) {
                if let Ok(mut transform) = joints.get_mut(tongue) {
                    let cycle = (a.time * 0.6 + a.seed).fract();
                    let flick = if cycle < 0.18 {
                        (cycle / 0.18 * PI * 3.0).sin().abs()
                    } else {
                        0.0
                    };
                    let out = (flick + coil).min(1.0) * (1.0 - d);
                    transform.scale = Vec3::new(1.0, 1.0, out.max(0.01));
                }
            }
        }
        Plan::Floater => {
            let d = dead.unwrap_or(0.0);
            let flare = match m {
                Some(_) => envelope(t, 0.0, 0.4, 0.5, 1.0),
                None => 0.0,
            };
            let dash = match m {
                Some(m) if m.offensive() => envelope(t, 0.38, 0.48, 0.55, 1.0),
                _ => 0.0,
            };
            if let Some(core) = rig.joint(J::Core) {
                if let Ok(mut transform) = joints.get_mut(core) {
                    transform.translation = Vec3::new(
                        0.0,
                        1.1 + (a.time * 2.2 + a.seed).sin() * 0.08 - 0.9 * d,
                        0.45 * dash,
                    );
                    transform.scale = Vec3::splat((1.0 + 0.35 * flare) * (1.0 - 0.6 * d));
                    transform.rotation = Quat::from_rotation_y(a.time * 0.7);
                }
            }
            for (j, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
                if let Some(e) = rig.joint(j) {
                    if let Ok(mut transform) = joints.get_mut(e) {
                        transform.rotation =
                            Quat::from_rotation_y(-side * 0.9 * dash - a.time * 0.7)
                                * Quat::from_rotation_z(
                                    side * (0.2 * (a.time * 1.3).sin() + 0.6 * flare),
                                );
                    }
                }
            }
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                let angle = a.time * (1.4 + f * 0.17) + f * TAU / 6.0;
                let r = (0.32 + 0.25 * flare) * (1.0 - d);
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = Vec3::new(
                        angle.cos() * r,
                        1.1 + (a.time * 3.0 + f).sin() * 0.25 - 0.9 * d,
                        angle.sin() * r + 0.45 * dash,
                    );
                    transform.scale = Vec3::splat(1.0 - d);
                }
            }
        }
        _ => {}
    }
}
