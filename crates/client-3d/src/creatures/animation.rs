use super::anatomy::{FOOT_SCALE, HIP_SCALE};
use super::geometry::{snake_segment_radius, BIRD_ANKLE, SNAKE};
use super::*;

mod rogue;

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
    mut rigs: Query<(&mut Rig, &mut Anim, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut joints: Query<&mut Transform, Without<Rig>>,
) {
    let dt = time.delta_secs().min(0.05);
    // Left alone for a minute, everyone starts to amuse themselves.
    let fun = bored.map_or(0.0, |b| ease((b.0 - BORED_AFTER) / 3.0));
    for (mut rig, mut anim, root) in &mut rigs {
        // Actor roots are unparented. Their current Transform already includes
        // this Update's interpolation; GlobalTransform still describes last frame.
        let at = root.translation;
        let first = anim.time == 0.0;
        let delta = at - anim.last;
        let moved = delta.with_y(0.0);
        anim.last = at;
        // The dead lie still, and a body just placed hasn't moved at all.
        // A jump of more than half a cell in one frame is a placement, not a walk.
        let placed = first || moved.length() > 0.5;
        let step = if placed || anim.dead.is_some() {
            0.0
        } else {
            moved.length()
        };
        let target_speed = (step / dt.max(1e-4) / (4.0 * rig.scale.max(0.2))).clamp(0.0, 1.0);
        anim.speed += (target_speed - anim.speed) * (1.0 - (-dt * 10.0).exp());
        // Fliers never touch the floor; a rooted plant never steps.
        let supported = matches!(
            rig.plan,
            Plan::Biped | Plan::Bird | Plan::Quadruped | Plan::Centaur
        ) && !rig.style.flies()
            && !placed
            && delta.y.abs() <= 0.001
            && !anim.quiet
            && anim.action.is_none()
            && anim.dead.is_none();
        // A flinch or a sidestep keeps the feet where they stand: dropping
        // their support would snap them from the floor in a single frame.
        if !supported || anim.contacts.rig != rig.joint(J::Lift) {
            anim.contacts = WalkContacts::default();
        }
        // Shorter slow steps need a correspondingly shorter distance cycle.
        // The support solve itself is not weakened a second time by speed.
        let stride = if supported {
            walk_stride(anim.speed)
        } else {
            1.0
        };
        anim.walk += step * TAU * 0.9 * rig.stride / (rig.scale.max(0.2) * stride);
        if step > 0.002 && anim.action.is_none() {
            anim.target_yaw = Some(moved.x.atan2(moved.z));
        }
        if anim.dead.is_some() {
            anim.target_yaw = None;
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
            anim.fallen = *d;
        } else {
            // A revived creature gets up over most of a second.
            anim.fallen = (anim.fallen - dt / 0.8).max(0.0);
        }
        let moving = step > dt * 0.04 * rig.scale.max(0.2);
        let pose = pose(&rig, &mut anim, fun, dt, supported, moving);
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
        let Rig { rest, robe, .. } = &mut *rig;
        if let Some(robe) = robe {
            super::robe::place(robe, rest, &pose.rot, &pose.off, &mut meshes);
        }
        place_segments(&rig, &anim, fun, &mut joints);
        swing_drapes(&rig, &anim, &mut joints);
    }
}

/// Hanging cloth follows its wearer with a little lag: a cloak flares back
/// as they travel and trails through a turn; skirts and rags sway with the
/// stride, longer cloth less far. At rest it barely stirs; the dead's lies still.
fn swing_drapes(rig: &Rig, a: &Anim, joints: &mut Query<&mut Transform, Without<Rig>>) {
    if rig.drapes.is_empty() {
        return;
    }
    let alive = if a.dead.is_none() { 1.0 } else { 0.0 };
    let walk = ease(a.speed * 4.0) * alive;
    let turning = a.target_yaw.map_or(0.0, |goal| {
        ((goal - a.yaw + PI).rem_euclid(TAU) - PI).clamp(-1.0, 1.0)
    }) * alive;
    let stir = (a.time * 1.3 + a.seed).sin() * alive;
    let stride = a.walk.sin();
    for &(e, kind) in &rig.drapes {
        let rotation = match kind {
            Drape::Cloak => {
                Quat::from_rotation_x(0.3 * walk + 0.05 * stride.abs() * walk + 0.012 * stir)
                    * Quat::from_rotation_z(0.3 * turning + 0.04 * stride * walk)
            }
            Drape::Skirt(length) => {
                let give = 1.0 / (1.0 + 2.5 * length);
                Quat::from_rotation_z((0.09 * stride * walk + 0.2 * turning) * give)
                    * Quat::from_rotation_x((0.1 * walk + 0.008 * stir) * give)
            }
        };
        if let Ok(mut transform) = joints.get_mut(e) {
            transform.rotation = rotation;
        }
    }
}

fn pose(rig: &Rig, a: &mut Anim, fun: f32, dt: f32, supported: bool, moving: bool) -> Pose {
    let mut p = Pose::new();
    // Once a fall starts, ordinary breathing and gait cannot move the body
    // underneath its death pose. The fall itself is the only moving layer.
    let alive = if a.dead.is_none() { 1.0 } else { 0.0 };
    let s = a.speed * alive;
    let w = a.walk * alive;
    let breath = (a.time * 1.8 + a.seed).sin() * alive;
    let hurt = a
        .hurt
        .filter(|t| *t >= 0.0)
        .map_or(0.0, |t| (1.0 - t / 0.4) * (t / 0.06).min(1.0));
    let dodge = a
        .dodge
        .filter(|t| *t >= 0.0)
        .map_or(0.0, |t| (t / 0.45 * PI).sin());
    let (m, t) = a
        .action
        .filter(|_| a.dead.is_none())
        .map_or((None, 0.0), |(m, t)| (Some(m), t));
    // At rest each creature idles in its own way, fading out as it moves or acts.
    // Fidgets belong to rest. Normal travelling speeds already have a full
    // gait even though speed is normalized against a much faster run.
    let settled = f32::from(u8::from(m.is_none() && a.dead.is_none()));
    // Idle fidgets fade out as an action begins and back in after it ends,
    // rather than switching off and on in a single frame mid-gesture.
    a.rest += (settled - a.rest) * (1.0 - (-dt * 7.0).exp());
    let calm = (1.0 - ease(s * 4.0).max(a.contacts.blend)) * a.rest;
    if calm > 0.0 && !a.quiet {
        if !rogue::idle(&mut p, rig, a.time + a.seed * 3.7, a.seed, calm) {
            idle(&mut p, rig, a.time + a.seed * 3.7, a.seed, calm);
        }
        if fun > 0.0 {
            let (w, gag, progress) = antic(a);
            if w > 0.0 && !rogue::antics(&mut p, rig, a.time, gag, progress, w * calm * fun) {
                antics(&mut p, rig, a.time, gag, progress, w * calm * fun);
            }
        }
    }
    let world_walk = supported && (moving || a.contacts.rig.is_some());
    match rig.plan {
        Plan::Biped => biped_pose(&mut p, rig, s, w, breath, m, t),
        Plan::Bird if rig.style.flies() => rogue::flier_pose(&mut p, rig, a, s, breath, m, t),
        Plan::Bird => bird_pose(&mut p, s, w, breath, m, t, !world_walk),
        Plan::Quadruped | Plan::Centaur => quad_pose(&mut p, rig, s, w, breath, m, t, !world_walk),
        Plan::Rooted => rogue::rooted_pose(&mut p, a, breath, m, t),
        Plan::Serpent | Plan::Floater | Plan::Marker => {}
    }
    if world_walk {
        grounded_world_walk(&mut p, rig, a, dt, moving);
    } else if rig.plan == Plan::Biped && a.dead.is_none() {
        if let Some(m) = m {
            grounded_action(&mut p, rig, a, m, t);
        } else if a.hurt.is_none() && a.dodge.is_none() && s > 0.0 {
            grounded_walk(&mut p, rig, a);
        }
    }
    // Reactions, shared by every plan.
    p.r(J::Lift, Quat::from_rotation_x(-0.28 * hurt));
    p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * hurt));
    p.r(J::Head, bow(-0.35 * hurt));
    p.r(J::Neck, bow(-0.3 * hurt));
    let side = if a.seed > 5.0 { 1.0 } else { -1.0 };
    p.o(J::Lift, Vec3::new(side * 0.28 * dodge, 0.0, -0.05 * dodge));
    p.r(J::Lift, Quat::from_rotation_z(-side * 0.25 * dodge));
    if let Some(d) = a.dead.or((a.fallen > 0.0).then_some(a.fallen)) {
        death_pose(&mut p, rig, ease(d), a.seed);
    }
    p
}

/// Shared contact timing for legs, pelvis and arms. Family differences change
/// the support duration and pickup, without changing distance-driven foot speed.
struct BipedGait {
    cadence: f32,
    stance: f32,
    clearance: f32,
    crouch: f32,
    arm: f32,
    bounce: f32,
}

fn biped_gait(style: Style) -> BipedGait {
    let (cadence, stance, clearance, crouch, arm, bounce) = match style {
        Style::Guardian => (1.4, 0.67, 0.06, 0.085, 0.12, 0.008),
        Style::Yeti => (1.25, 0.65, 0.075, 0.1, 0.18, 0.012),
        Style::Zombie => (1.5, 0.67, 0.055, 0.11, 0.0, 0.009),
        Style::Hobgoblin => (1.15, 0.6, 0.095, 0.095, 0.27, 0.014),
        Style::Vampire => (1.18, 0.63, 0.065, 0.07, 0.18, 0.006),
        Style::Caster => (1.0, 0.57, 0.08, 0.075, 0.18, 0.011),
        Style::Sentinel => (1.0, 0.57, 0.09, 0.075, 0.2, 0.011),
        Style::Scout => (1.05, 0.55, 0.11, 0.08, 0.3, 0.013),
        Style::Orc => (1.12, 0.6, 0.09, 0.09, 0.26, 0.014),
        Style::Troll => (1.3, 0.64, 0.07, 0.1, 0.15, 0.012),
        Style::Leprechaun => (0.92, 0.52, 0.12, 0.07, 0.32, 0.018),
        Style::Nymph => (0.96, 0.53, 0.12, 0.07, 0.3, 0.016),
        Style::Medusa => (1.1, 0.6, 0.07, 0.07, 0.15, 0.008),
        // The dead glide, barely lifting their feet.
        Style::Phantom => (1.2, 0.62, 0.04, 0.05, 0.05, 0.004),
        Style::Wraith => (1.25, 0.63, 0.04, 0.05, 0.04, 0.004),
        _ => (1.0, 0.55, 0.11, 0.075, 0.28, 0.014),
    };
    BipedGait {
        cadence,
        stance,
        clearance,
        crouch,
        arm,
        bounce,
    }
}

const BIPED_LEGS: [(J, J, J); 2] = [(J::LegL, J::ShinL, J::FootL), (J::LegR, J::ShinR, J::FootR)];

fn biped_rest_contacts(rig: &Rig) -> [Vec3; 2] {
    BIPED_LEGS.map(|(leg, shin, foot)| {
        rig.rest[J::Hips as usize]
            + rig.rest[leg as usize]
            + rig.rest[shin as usize]
            + rig.rest[foot as usize]
    })
}

fn biped_walk_contacts(rig: &Rig, a: &Anim) -> [Vec3; 2] {
    let gait = biped_gait(rig.style);
    let blend = ease(a.speed * 4.0);
    let mut contacts = biped_rest_contacts(rig);
    for (i, (leg, _, _)) in BIPED_LEGS.into_iter().enumerate() {
        let drag = rig.style == Style::Zombie && leg == J::LegR;
        let stance = gait.stance + if drag { 0.06 } else { 0.0 };
        let clearance = if drag { 0.018 } else { gait.clearance };
        let span = stance / (0.9 * rig.stride.max(0.1) * gait.cadence);
        let phase = a.walk * gait.cadence / TAU + i as f32 * 0.5;
        let (z, lift) = beast_step(phase, stance, span, clearance);
        contacts[i] += Vec3::new(0.0, lift, z) * blend;
    }
    contacts
}

/// Ankles follow a distance-driven support/return path. The same small solve
/// also supports action contacts; there is no separate set of attack leg lengths.
fn grounded_walk(p: &mut Pose, rig: &Rig, a: &Anim) {
    let gait = biped_gait(rig.style);
    let blend = ease(a.speed * 4.0);
    p.off[J::Lift as usize].y -= gait.crouch * blend;
    solve_biped_contacts(p, rig, &biped_walk_contacts(rig, a), blend);
}

fn walk_stride(speed: f32) -> f32 {
    ease(speed * 4.0).max(0.2)
}

#[derive(Clone, Copy)]
struct WalkLeg {
    body: J,
    thigh: J,
    shin: J,
    foot: J,
    rest: Vec3,
    floor: f32,
    phase: f32,
    stance: f32,
    span: f32,
    clearance: f32,
    bend: f32,
}

/// The existing family timing, expressed once for the shared support memory.
fn walk_legs(rig: &Rig, a: &Anim) -> [Option<WalkLeg>; 4] {
    let mut legs = [None; 4];
    let mut add = |i: usize,
                   body: J,
                   thigh: J,
                   shin: J,
                   foot: J,
                   phase: f32,
                   stance: f32,
                   cadence: f32,
                   clearance: f32,
                   bend: f32,
                   floor: Option<f32>| {
        if rig.joint(foot).is_some() {
            let rest = rig.rest[body as usize]
                + rig.rest[thigh as usize]
                + rig.rest[shin as usize]
                + rig.rest[foot as usize];
            legs[i] = Some(WalkLeg {
                body,
                thigh,
                shin,
                foot,
                rest,
                floor: floor.unwrap_or(rest.y),
                phase,
                stance,
                span: stance / (0.9 * rig.stride.max(0.1) * cadence),
                clearance,
                bend,
            });
        }
    };
    match rig.plan {
        Plan::Biped => {
            let gait = biped_gait(rig.style);
            for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
                let drag = rig.style == Style::Zombie && i == 1;
                add(
                    i,
                    J::Hips,
                    leg,
                    shin,
                    foot,
                    a.walk * gait.cadence / TAU + i as f32 * 0.5,
                    gait.stance + if drag { 0.06 } else { 0.0 },
                    gait.cadence,
                    if drag { 0.018 } else { gait.clearance },
                    1.0,
                    None,
                );
            }
        }
        Plan::Bird => {
            for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
                add(
                    i,
                    J::Body,
                    leg,
                    shin,
                    foot,
                    a.walk * 1.6 / TAU + i as f32 * 0.5,
                    0.6,
                    1.6,
                    0.1,
                    1.0,
                    Some(0.042),
                );
            }
        }
        Plan::Quadruped | Plan::Centaur => {
            let trot = ease((a.speed - 0.38) / 0.45);
            let legs = rig.legs;
            for (i, (leg, shin, foot, offset)) in [
                (J::FL, J::FLs, J::HoofFL, 0.0),
                (J::FR, J::FRs, J::HoofFR, 0.5),
                (J::BL, J::BLs, J::HoofBL, 0.25 + 0.25 * trot),
                (J::BR, J::BRs, J::HoofBR, -0.25 + 0.25 * trot),
            ]
            .into_iter()
            .enumerate()
            {
                add(
                    i,
                    J::Body,
                    leg,
                    shin,
                    foot,
                    a.walk * 1.35 / TAU + offset,
                    0.69 - 0.12 * trot,
                    1.35,
                    if rig.style == Style::Unicorn {
                        0.11
                    } else {
                        0.075 * legs
                    },
                    if i < 2 { 1.0 } else { -1.0 },
                    Some(rig.ankle_floor),
                );
            }
        }
        _ => {}
    }
    legs
}

fn heading_between(from: f32, to: f32, weight: f32) -> f32 {
    from + ((to - from + PI).rem_euclid(TAU) - PI) * weight
}

fn walk_hip(p: &Pose, rig: &Rig, leg: WalkLeg) -> Vec3 {
    p.rot[J::Lift as usize]
        * (rig.rest[leg.body as usize]
            + p.off[leg.body as usize]
            + p.rot[leg.body as usize] * (rig.rest[leg.thigh as usize] + p.off[leg.thigh as usize]))
        + p.off[J::Lift as usize]
}

fn posed_ankle(p: &Pose, rig: &Rig, leg: WalkLeg) -> Vec3 {
    let thigh = p.rot[J::Lift as usize] * p.rot[leg.body as usize] * p.rot[leg.thigh as usize];
    walk_hip(p, rig, leg)
        + thigh
            * (rig.rest[leg.shin as usize]
                + p.off[leg.shin as usize]
                + p.rot[leg.shin as usize]
                    * (rig.rest[leg.foot as usize] + p.off[leg.foot as usize]))
}

/// Support holds a world point. Swing releases it along a clear arc and
/// approaches the next landing with the nominal backwards contact velocity.
fn walking_target(a: &Anim, leg: WalkLeg, foot: WalkFoot, scale: f32) -> (Vec3, f32) {
    let facing = Quat::from_rotation_y(a.yaw);
    let from = facing.inverse() * (foot.world - a.last) / scale;
    let phase = leg.phase.rem_euclid(1.0);
    if phase < leg.stance {
        return (from, foot.yaw);
    }
    let cycle = leg.phase.floor();
    let start = foot.lifted.max(cycle + leg.stance).min(leg.phase);
    let duration = (cycle + 1.0 - start).max(0.001);
    let t = ((leg.phase - start) / duration).clamp(0.0, 1.0);
    let stride = walk_stride(a.speed);
    let to = leg.rest.with_y(leg.floor) + Vec3::Z * (leg.span * stride * 0.5);
    let mut target = from.lerp(to, ease(t));
    target.y += leg.clearance * stride * (PI * t).sin().powi(2);
    target.z -= leg.span * stride * duration / leg.stance * t * t * (1.0 - t) * (1.0 - 2.0 * t);
    (target, heading_between(foot.yaw, a.yaw, ease(t)))
}

fn settle_progress(foot: WalkFoot, elapsed: f32, count: usize) -> f32 {
    let duration = if count == 2 { 0.16 } else { 0.12 };
    ((elapsed - f32::from(foot.settle_order) * duration) / duration).clamp(0.0, 1.0)
}

fn settling_target(
    a: &Anim,
    leg: WalkLeg,
    foot: WalkFoot,
    elapsed: f32,
    count: usize,
    scale: f32,
) -> (Vec3, f32) {
    let from = Quat::from_rotation_y(-a.yaw) * (foot.world - a.last) / scale;
    if foot.settled {
        return (from, foot.yaw);
    }
    let t = settle_progress(foot, elapsed, count);
    let to = leg.rest.with_y(leg.floor);
    let mut target = from.lerp(to, ease(t));
    let pickup = (from - to).with_y(0.0).length().min(leg.clearance * 0.6);
    target.y += pickup * (PI * t).sin().powi(2);
    (target, heading_between(foot.yaw, a.yaw, ease(t)))
}

/// Confine a contact before IK, retaining its floor height. Extreme turns may
/// yield a little instead of crossing legs, stretching, or retaining a far anchor.
fn reachable_walk_contact(p: &Pose, rig: &Rig, leg: WalkLeg, mut target: Vec3) -> Vec3 {
    let hip = walk_hip(p, rig, leg);
    let upper = rig.rest[leg.shin as usize].length();
    let reach = upper + rig.rest[leg.foot as usize].length() - 0.002;
    target.x = target
        .x
        .clamp(leg.rest.x - upper * 0.32, leg.rest.x + upper * 0.32);
    let across = (target - hip).with_y(0.0);
    let radius = (reach * reach - (target.y - hip.y).powi(2)).max(0.0).sqrt();
    if across.length_squared() > radius * radius {
        let bounded = hip + across.normalize_or(Vec3::Z) * radius;
        target.x = bounded.x;
        target.z = bounded.z;
    }
    target
}

fn grounded_world_walk(p: &mut Pose, rig: &Rig, a: &mut Anim, dt: f32, moving: bool) {
    let mut legs = walk_legs(rig, a);
    if a.contacts.settle.is_none() {
        // Changing the equine walk/trot offset during braking may otherwise
        // run a hind foot's phase backwards across a touchdown.
        for (leg, foot) in legs.iter_mut().zip(a.contacts.feet) {
            if let (Some(leg), Some(foot)) = (leg, foot) {
                leg.phase = leg.phase.max(foot.phase);
            }
        }
    }
    let count = legs.iter().flatten().count();
    if count == 0 {
        a.contacts = WalkContacts::default();
        return;
    }
    let facing = Quat::from_rotation_y(a.yaw);
    let scale = rig.scale.max(0.2);
    let seeded = a.contacts.rig.is_some();
    if moving && (!seeded || a.contacts.settle.is_some()) {
        for (i, leg) in legs
            .into_iter()
            .enumerate()
            .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
        {
            let (at, yaw) = if let (Some(t), Some(foot)) = (a.contacts.settle, a.contacts.feet[i]) {
                settling_target(a, leg, foot, t, count, scale)
            } else {
                (posed_ankle(p, rig, leg).with_y(leg.floor), a.yaw)
            };
            a.contacts.feet[i] = Some(WalkFoot {
                world: a.last + facing * at * scale,
                yaw,
                phase: leg.phase,
                lifted: leg.phase,
                settle_order: 0,
                settled: false,
            });
        }
        a.contacts.rig = rig.joint(J::Lift);
        a.contacts.settle = None;
    } else if !moving && a.contacts.settle.is_none() {
        for (i, leg) in legs
            .into_iter()
            .enumerate()
            .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
        {
            if let Some(mut foot) = a.contacts.feet[i] {
                let (at, yaw) = walking_target(a, leg, foot, scale);
                foot.world = a.last + facing * at * scale;
                foot.yaw = yaw;
                // Land the feet already in swing before moving any support.
                let airborne = leg.phase.rem_euclid(1.0) >= leg.stance;
                foot.settle_order = legs
                    .iter()
                    .enumerate()
                    .filter(|(j, other)| {
                        other.is_some_and(|other| {
                            let other_airborne = other.phase.rem_euclid(1.0) >= other.stance;
                            (other_airborne && !airborne) || (other_airborne == airborne && *j < i)
                        })
                    })
                    .count() as u8;
                foot.settled = false;
                a.contacts.feet[i] = Some(foot);
            }
        }
        a.contacts.settle = Some(0.0);
    }
    if moving {
        a.contacts.blend = (a.contacts.blend + dt / 0.12).min(1.0);
    } else if let Some(t) = a.contacts.settle.as_mut() {
        *t += dt;
        let end = if count == 2 { 0.32 } else { 0.48 };
        if *t > end {
            a.contacts.blend = (a.contacts.blend - dt / 0.14).max(0.0);
        }
    }
    let weight = ease(a.contacts.blend);
    if weight == 0.0 {
        a.contacts = WalkContacts::default();
        return;
    }
    let stride = walk_stride(a.speed);
    let procedural = ease(a.speed * 4.0);
    match rig.plan {
        Plan::Biped => p.off[J::Lift as usize].y -= biped_gait(rig.style).crouch * stride * weight,
        Plan::Bird => p.off[J::Lift as usize].y -= 0.09 * (stride - procedural) * weight,
        _ => p.off[J::Lift as usize].y -= 0.113 * rig.legs * (stride - procedural) * weight,
    }
    let mut targets = [Vec3::ZERO; 4];
    let mut headings = [a.yaw; 4];
    for (i, leg) in legs
        .into_iter()
        .enumerate()
        .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
    {
        let Some(mut foot) = a.contacts.feet[i] else {
            continue;
        };
        if a.contacts.settle.is_none() {
            let crossed = leg.phase.floor() != foot.phase.floor();
            if leg.phase.rem_euclid(1.0) < leg.stance {
                if crossed || foot.phase.rem_euclid(1.0) >= leg.stance {
                    let phase = leg.phase.rem_euclid(1.0);
                    let at = leg.rest.with_y(leg.floor)
                        + Vec3::Z * (leg.span * stride * (0.5 - phase / leg.stance));
                    foot.world = a.last + facing * at * scale;
                    foot.yaw = a.yaw;
                }
            } else if crossed || foot.phase.rem_euclid(1.0) < leg.stance {
                foot.lifted = leg.phase.floor() + leg.stance;
            }
        }
        let (target, yaw) = if let Some(t) = a.contacts.settle {
            settling_target(a, leg, foot, t, count, scale)
        } else {
            walking_target(a, leg, foot, scale)
        };
        targets[i] = target;
        headings[i] = a.yaw + ((yaw - a.yaw + PI).rem_euclid(TAU) - PI).clamp(-0.85, 0.85);
        if let Some(t) = a.contacts.settle {
            if !foot.settled && settle_progress(foot, t, count) >= 1.0 {
                foot.world = a.last + facing * target * scale;
                foot.yaw = headings[i];
                foot.settled = true;
            }
        }
        foot.phase = leg.phase;
        a.contacts.feet[i] = Some(foot);
    }
    if rig.plan == Plan::Biped {
        // Bound a displaced anchor before fitting the pelvis. A tight turn
        // must not demand a deep squat merely to retain an unreachable point.
        for i in 0..2 {
            if let Some(leg) = legs[i] {
                targets[i] = reachable_walk_contact(p, rig, leg, targets[i]);
            }
        }
        fit_biped_support(p, rig, &[targets[0], targets[1]], weight);
    }
    for (i, leg) in legs
        .into_iter()
        .enumerate()
        .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
    {
        let target = reachable_walk_contact(p, rig, leg, targets[i]);
        let planted = a.contacts.settle.is_none() && leg.phase.rem_euclid(1.0) < leg.stance;
        if planted || a.contacts.feet[i].is_some_and(|foot| foot.settled) {
            if let Some(foot) = a.contacts.feet[i].as_mut() {
                foot.world = a.last + facing * target * scale;
                foot.yaw = headings[i];
            }
        }
        if rig.plan == Plan::Biped {
            targets[i] = target;
        } else {
            beast_limb(
                p,
                leg.thigh,
                leg.shin,
                leg.foot,
                rig.rest[leg.body as usize],
                rig.rest[leg.thigh as usize],
                rig.rest[leg.shin as usize],
                rig.rest[leg.foot as usize],
                target,
                leg.bend,
                weight,
            );
        }
    }
    if rig.plan == Plan::Biped {
        solve_biped_contacts(p, rig, &[targets[0], targets[1]], weight);
    }
    for (i, leg) in legs
        .into_iter()
        .enumerate()
        .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
    {
        p.r(
            leg.foot,
            Quat::from_rotation_y((headings[i] - a.yaw) * weight),
        );
    }
}

fn solve_biped_contacts(p: &mut Pose, rig: &Rig, contacts: &[Vec3; 2], blend: f32) {
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        if rig.joint(foot).is_none() {
            continue;
        }
        let rest_hip = rig.rest[J::Hips as usize];
        let rest_leg = rig.rest[leg as usize];
        let upper = rig.rest[shin as usize].length();
        let lower = rig.rest[foot as usize].length();
        let root = p.rot[J::Lift as usize];
        let hips = p.rot[J::Hips as usize];
        let target = hips.inverse()
            * (root.inverse() * (contacts[i] - p.off[J::Lift as usize])
                - rest_hip
                - p.off[J::Hips as usize])
            - rest_leg
            - p.off[leg as usize];
        let distance = target
            .length()
            .clamp((upper - lower).abs() + 0.001, upper + lower - 0.001);
        let direction = target.normalize_or(Vec3::NEG_Y);
        let along = (upper * upper - lower * lower + distance * distance) / (2.0 * distance);
        let bend = (upper * upper - along * along).max(0.0).sqrt();
        let forward = (Vec3::Z - direction * direction.dot(Vec3::Z)).normalize_or(Vec3::X);
        let knee = direction * along + forward * bend;
        let thigh = Quat::from_rotation_arc(Vec3::NEG_Y, knee.normalize_or(Vec3::NEG_Y));
        let calf = Quat::from_rotation_arc(
            Vec3::NEG_Y,
            (direction * distance - knee).normalize_or(Vec3::NEG_Y),
        );
        // Fade right to zero so both travel and actions can give the individual
        // idle pose back without snapping a knee at a speed/timer threshold.
        p.rot[leg as usize] = p.rot[leg as usize].slerp(thigh, blend);
        p.rot[shin as usize] = p.rot[shin as usize].slerp(thigh.inverse() * calf, blend);
        let actual = root * hips * p.rot[leg as usize] * p.rot[shin as usize];
        p.rot[foot as usize] = p.rot[foot as usize].slerp(actual.inverse(), blend);
    }
}

/// Keep both requested contacts reachable before solving the legs. A visual
/// forward drive can otherwise put the rear ankle outside the leg's sphere,
/// and clamping the IK alone would leave its sole floating above the floor.
fn fit_biped_support(p: &mut Pose, rig: &Rig, contacts: &[Vec3; 2], weight: f32) {
    let root = p.rot[J::Lift as usize];
    let hips = p.rot[J::Hips as usize];
    let mut drop = 0.0f32;
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        if rig.joint(foot).is_none() {
            continue;
        }
        let at = root
            * (rig.rest[J::Hips as usize]
                + p.off[J::Hips as usize]
                + hips * (rig.rest[leg as usize] + p.off[leg as usize]))
            + p.off[J::Lift as usize];
        let reach = rig.rest[shin as usize].length() + rig.rest[foot as usize].length() - 0.006;
        let across = (contacts[i] - at).with_y(0.0).length_squared();
        let height = (reach * reach - across).max(0.0).sqrt();
        drop = drop.max(at.y - contacts[i].y - height);
    }
    p.off[J::Lift as usize].y -= drop * weight;
    if rig.style == Style::Caster {
        limit_caster_support(p, rig, contacts);
    }
}

/// Retain the caster's conservative floor-length rest-hem support limit.
/// Account for the whole hip chain, then shorten only an unreachable body
/// drive. Articulated cloth follows the resulting final pose independently.
fn limit_caster_support(p: &mut Pose, rig: &Rig, contacts: &[Vec3; 2]) {
    let root = p.rot[J::Lift as usize];
    let hips = p.rot[J::Hips as usize];
    let up = (root * hips).inverse() * Vec3::Y;
    let body = rig.humanoid.expect("biped support has resolved anatomy");
    let build = body.build;
    let hip_width = body.build * body.hips_width;
    let flare = 0.208 + 0.84 * 0.045;
    let hem_x = (flare * hip_width + 0.009) * HIP_SCALE.x;
    let hem_z = (flare * 0.74 * build + 0.009) * HIP_SCALE.z;
    let origin =
        root * (rig.rest[J::Hips as usize] + p.off[J::Hips as usize]) + p.off[J::Lift as usize];
    let hem_y = origin.y - 0.84 * up.y - hem_x * up.x.abs() - hem_z * up.z.abs();
    let raise = (0.025 - hem_y).max(0.0);
    if raise == 0.0 {
        return;
    }
    p.off[J::Lift as usize].y += raise;
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        let at = root
            * (rig.rest[J::Hips as usize]
                + p.off[J::Hips as usize]
                + hips * (rig.rest[leg as usize] + p.off[leg as usize]))
            + p.off[J::Lift as usize];
        let reach = rig.rest[shin as usize].length() + rig.rest[foot as usize].length() - 0.006;
        let across = (at.x - contacts[i].x).powi(2) + (at.y - contacts[i].y).powi(2);
        let span = (reach * reach - across).max(0.0).sqrt();
        let center = contacts[i].z - (at.z - p.off[J::Lift as usize].z);
        near = near.max(center - span);
        far = far.min(center + span);
    }
    if near <= far {
        p.off[J::Lift as usize].z = p.off[J::Lift as usize].z.clamp(near, far);
    }
}

/// A lead step has an explicit landing and a separate recovery pickup. Its
/// planted middle never follows the moving pelvis, and it returns exactly home.
fn action_step(t: f32, impact: f32) -> (f32, f32) {
    let land = (impact - 0.08).max(0.14);
    let recover = (impact + 0.38).clamp(0.78, 0.86);
    contact_step(t, 0.035, land, recover, 0.985)
}

fn contact_step(t: f32, start: f32, land: f32, recover: f32, end: f32) -> (f32, f32) {
    if t < start || t >= end {
        (0.0, 0.0)
    } else if t < land {
        let phase = (t - start) / (land - start);
        (ease(phase), (PI * phase).sin().powi(2))
    } else if t < recover {
        (1.0, 0.0)
    } else {
        let phase = (t - recover) / (end - recover);
        (1.0 - ease(phase), (PI * phase).sin().powi(2))
    }
}

/// Downward extent of the existing terminal foot after a toe-down turn.
/// The ellipse support formula bounds its sampled blobs; the stone foot uses
/// its box envelope. This includes the mesh-only 1.22 toe/sole depth scale.
fn biped_foot_depth(rig: &Rig, angle: f32) -> f32 {
    let (s, c) = angle.sin_cos();
    if rig.style == Style::Guardian {
        return 0.075 * c + 0.21 * FOOT_SCALE.z * s;
    }
    let body = rig.humanoid.expect("biped support has resolved anatomy");
    let mut depth = body
        .feet
        .into_iter()
        .fold(f32::NEG_INFINITY, |depth, (center, radii)| {
            let z = center.z * FOOT_SCALE.z;
            let rz = radii.z * FOOT_SCALE.z;
            depth.max(-center.y * c + z * s + Vec2::new(radii.y * c, rz * s).length())
        });
    if rig.style == Style::Yeti {
        depth = depth
            .max(0.04 * c + 0.17 * FOOT_SCALE.z * s + 0.012 * FOOT_SCALE.z)
            .max(0.06 * c + 0.22 * FOOT_SCALE.z * s);
    }
    depth
}

/// Plant the front foot before carrying the rear ankle back, then reverse
/// that order to rise. At full depth the rear knee and tucked toe support the
/// body; a two-flat-sole solve alone would replace the kneel with a squat.
fn mend_contacts(p: &mut Pose, rig: &Rig, t: f32, weight: f32) -> ([Vec3; 2], f32) {
    let mut contacts = biped_rest_contacts(rig);
    let kneel = envelope(t, 0.0, 0.25, 0.8, 1.0);
    if rig.style == Style::Caster {
        // Upright channeling retains the caster's restrained lower-body pose.
        // The articulated garment then follows the final supported legs.
        p.off[J::Lift as usize].y =
            p.off[J::Lift as usize].y * (1.0 - weight) - 0.025 * kneel * weight;
        return (contacts, 0.0);
    }
    let build = rig
        .humanoid
        .expect("biped support has resolved anatomy")
        .build;
    let knee_height = match rig.style {
        Style::Guardian => 0.104,
        Style::Sentinel => (0.076 * build + 0.003).max(0.104),
        Style::Yeti => 0.08 * build + 0.003,
        _ => 0.076 * build + 0.003,
    };
    let upper = rig.rest[J::ShinR as usize].length();
    let lower = rig.rest[J::FootR as usize].length();
    let knee_z = -0.06f32;
    let tuck = 1.05;
    let neutral_depth = biped_foot_depth(rig, 0.0);
    let tucked_y = contacts[1].y + biped_foot_depth(rig, tuck) - neutral_depth + 0.003;
    let rear_z = knee_z
        - (lower * lower - (tucked_y - knee_height).powi(2))
            .max(0.0)
            .sqrt();
    let (front, front_lift) = contact_step(t, 0.015, 0.14, 0.9, 0.985);
    let (rear, rear_lift) = contact_step(t, 0.145, 0.25, 0.8, 0.9);
    let family = if rig.style == Style::Zombie { 0.8 } else { 1.0 };
    contacts[0] += Vec3::new(0.0, 0.045 * front_lift, 0.3 * family * front);
    contacts[1].z += rear_z * rear;
    let foot_angle = tuck * rear;
    contacts[1].y +=
        biped_foot_depth(rig, foot_angle) - neutral_depth + 0.003 * rear + 0.065 * rear_lift;

    // Keep the authored .4-unit descent, adapting its settled depth only for
    // the knee's actual mass. Remove residual travel roll while it bears weight.
    p.rot[J::Lift as usize] = p.rot[J::Lift as usize].slerp(Quat::IDENTITY, kneel * weight);
    let root = p.rot[J::Lift as usize];
    let hips = p.rot[J::Hips as usize];
    let rest_hip = rig.rest[J::Hips as usize] + rig.rest[J::LegR as usize];
    let at = root
        * (rig.rest[J::Hips as usize]
            + p.off[J::Hips as usize]
            + hips * (rig.rest[J::LegR as usize] + p.off[J::LegR as usize]))
        + p.off[J::Lift as usize];
    let across = (Vec3::new(contacts[1].x, knee_height, knee_z) - at)
        .with_y(0.0)
        .length_squared();
    let supported_hip = knee_height + (upper * upper - across).max(0.0).sqrt();
    let wanted = rest_hip.y + (supported_hip - rest_hip.y) * kneel;
    p.off[J::Lift as usize].y += (wanted - at.y) * weight;
    (contacts, foot_angle)
}

fn grounded_action(p: &mut Pose, rig: &Rig, a: &Anim, m: Move, t: f32) {
    let weight = envelope(t, 0.0, 0.08, 0.92, 1.0);
    if a.speed > 0.0 {
        grounded_walk(p, rig, a);
    }
    let walking = biped_walk_contacts(rig, a);
    if matches!(m, Move::Step | Move::Mend) {
        let (mut planned, foot_angle) = if m == Move::Mend {
            mend_contacts(p, rig, t, weight)
        } else {
            let mut contacts = biped_rest_contacts(rig);
            let (back, pickup) = contact_step(t, 0.035, 0.35, 0.72, 0.985);
            let hop = (PI * (t / 0.35).clamp(0.0, 1.0)).sin().max(0.0) * 0.05;
            contacts[0].y += hop;
            contacts[1] += Vec3::new(0.0, hop + 0.055 * pickup, -0.38 * back);
            (contacts, 0.0)
        };
        for i in 0..2 {
            planned[i] = walking[i].lerp(planned[i], weight);
        }
        fit_biped_support(p, rig, &planned, weight);
        solve_biped_contacts(p, rig, &planned, weight);
        p.r(J::FootR, bow(foot_angle * weight));
        return;
    }
    let (lead, stride): (usize, f32) = match m {
        Move::Strike => (0, 0.22),
        Move::Thrust => (0, 0.34),
        Move::Bash => (0, 0.32),
        Move::Shove => (0, 0.4),
        Move::Heavy => (0, 0.18),
        Move::Hook => (0, 0.14),
        Move::Touch(_) => (0, 0.25),
        Move::Bite(_) => (0, 0.2),
        Move::Combo(other) => (usize::from(other), 0.14),
        _ => (0, 0.0),
    };
    let mut planned = biped_rest_contacts(rig);
    let (advance, pickup) = action_step(t, m.impact());
    let (leg, shin, foot) = BIPED_LEGS[lead];
    let length = rig.rest[shin as usize].length() + rig.rest[foot as usize].length();
    let family = if rig.style == Style::Zombie { 0.8 } else { 1.0 };
    let stride = (stride * family).min(length * 0.48);
    if stride > 0.0 {
        let clearance = if matches!(rig.style, Style::Guardian | Style::Zombie) {
            0.035
        } else {
            0.055
        };
        let side = rig.rest[leg as usize].x.signum();
        planned[lead] += Vec3::new(side * 0.016 * advance, clearance * pickup, stride * advance);
    }
    let contacts = [
        walking[0].lerp(planned[0], weight),
        walking[1].lerp(planned[1], weight),
    ];
    fit_biped_support(p, rig, &contacts, weight);
    solve_biped_contacts(p, rig, &contacts, weight);
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
        Plan::Marker | Plan::Rooted => {}
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
        Plan::Serpent | Plan::Floater | Plan::Rooted | Plan::Marker => {}
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
    let gait = biped_gait(rig.style);
    let blend = ease(s * 4.0);
    let walking = m.is_none();
    let cycle = if walking { w * gait.cadence } else { w };
    // Contact phase zero is left-foot forward. Its opposite shoulder and arm
    // lead at the same time, rather than lagging the new foot path by a quarter.
    let arm_swing = if walking {
        cycle.cos() * gait.arm * blend
    } else {
        swing * 0.7
    };
    let twist = if walking {
        cycle.cos() * 0.08 * blend
    } else {
        w.sin() * 0.12 * s
    };
    let bob = if walking {
        -(cycle * 2.0).cos() * gait.bounce * blend
    } else {
        -(w * 2.0).cos() * 0.016 * s
    };
    p.o(J::Lift, Vec3::Y * bob);
    p.r(
        J::Torso,
        Quat::from_rotation_y(twist) * bow(rig.hunch + 0.08 * s + breath * 0.015),
    );
    if walking {
        p.r(
            J::Hips,
            Quat::from_rotation_y(-cycle.cos() * if lumber { 0.014 } else { 0.025 } * blend),
        );
    }
    if lumber {
        let roll = if walking {
            -cycle.cos() * 0.03 * blend
        } else {
            w.sin() * 0.06 * s
        };
        p.r(J::Lift, Quat::from_rotation_z(roll));
    }
    if zombie {
        // The arms stay reaching and the head lolls, while the right foot
        // drags through its longer, lower-clearance support/return path.
        p.r(
            J::ArmL,
            fwd(1.25 + breath * 0.05) * Quat::from_rotation_z(0.1),
        );
        p.r(
            J::ArmR,
            fwd(1.15 - breath * 0.05) * Quat::from_rotation_z(-0.1),
        );
        p.r(J::Head, Quat::from_rotation_z(0.25 + breath * 0.05));
        let roll = if walking {
            -cycle.cos() * 0.045 * blend
        } else {
            w.sin() * 0.1 * s
        };
        p.r(J::Lift, Quat::from_rotation_z(roll));
    } else {
        p.r(
            J::ArmL,
            fwd(-arm_swing) * Quat::from_rotation_z(0.1 + breath * 0.01),
        );
        p.r(
            J::ArmR,
            fwd(arm_swing) * Quat::from_rotation_z(-0.1 - breath * 0.01),
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
            // This is one guarded pickup, not a second sine lobe in recovery.
            let lift = (PI * (t / 0.35).clamp(0.0, 1.0)).sin().max(0.0);
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
            // Robed casters bind/channel upright; the remaining cast lowers
            // onto a supported knee, hands working a bandage round the arm.
            let kneel = envelope(t, 0.0, 0.25, 0.8, 1.0);
            let wrap = (t * 14.0).sin() * kneel;
            let upright = rig.style == Style::Caster;
            let descent = if upright { 0.025 } else { 0.4 };
            p.o(J::Lift, Vec3::Y * (-descent * kneel));
            if !upright {
                p.r(J::LegL, fwd(1.35 * kneel));
                p.r(J::ShinL, fwd(-1.45 * kneel));
                p.r(J::LegR, fwd(-0.15 * kneel));
                p.r(J::ShinR, fwd(-1.5 * kneel));
            }
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
            p.r(J::Torso, bow((if upright { 0.12 } else { 0.3 }) * kneel));
            p.r(J::Head, bow((if upright { 0.24 } else { 0.45 }) * kneel));
        }
        Move::Breath(_) | Move::Gaze | Move::Bite(_) => rogue::biped_move(p, m, t),
    }
}

/// A foot stays in support for most of a step, then clears the ground on
/// a shorter return arc. Its backwards speed agrees on both sides of contact.
fn beast_step(phase: f32, stance: f32, span: f32, clearance: f32) -> (f32, f32) {
    let phase = phase.rem_euclid(1.0);
    if phase < stance {
        (span * (0.5 - phase / stance), 0.0)
    } else {
        let t = (phase - stance) / (1.0 - stance);
        let carry = -span * (1.0 - stance) / stance * t * (1.0 - t) * (1.0 - 2.0 * t);
        (
            span * (ease(t) - 0.5) + carry,
            clearance * (PI * t).sin().powi(2),
        )
    }
}

/// Solve an animal ankle against its contact target in the lift frame, then
/// turn the terminal foot against the actual chain so its sole stays level.
/// The two bone lengths and ankle-to-sole profile come from the original mesh.
#[allow(clippy::too_many_arguments)]
fn beast_limb(
    p: &mut Pose,
    thigh: J,
    shin: J,
    foot: J,
    body_rest: Vec3,
    hip: Vec3,
    upper: Vec3,
    lower: Vec3,
    contact: Vec3,
    bend_side: f32,
    weight: f32,
) {
    if weight <= 0.0 {
        return;
    }
    let root = p.rot[J::Lift as usize];
    let body = p.rot[J::Body as usize];
    let target = body.inverse()
        * (root.inverse() * (contact - p.off[J::Lift as usize])
            - body_rest
            - p.off[J::Body as usize])
        - hip
        - p.off[thigh as usize];
    let upper_len = upper.length();
    let lower_len = lower.length();
    let distance = target.length().clamp(
        (upper_len - lower_len).abs() + 0.001,
        upper_len + lower_len - 0.001,
    );
    let direction = target.normalize_or(Vec3::NEG_Y);
    let along =
        (upper_len * upper_len - lower_len * lower_len + distance * distance) / (2.0 * distance);
    let bend = (upper_len * upper_len - along * along).max(0.0).sqrt();
    let forward = (Vec3::Z - direction * direction.dot(Vec3::Z)).normalize_or(Vec3::X);
    let knee = direction * along + forward * bend * bend_side;
    let upper_rot = Quat::from_rotation_arc(upper.normalize(), knee.normalize());
    let lower_rot = upper_rot.inverse()
        * Quat::from_rotation_arc(lower.normalize(), (direction * distance - knee).normalize());
    p.rot[thigh as usize] = p.rot[thigh as usize].slerp(upper_rot, weight);
    p.rot[shin as usize] = p.rot[shin as usize].slerp(lower_rot, weight);
    let actual = root * body * p.rot[thigh as usize] * p.rot[shin as usize];
    p.rot[foot as usize] = p.rot[foot as usize].slerp(actual.inverse(), weight);
}

#[allow(clippy::too_many_arguments)]
fn bird_pose(p: &mut Pose, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32, local_legs: bool) {
    let walk = ease(s * 4.0);
    // Keep the front contact inside both bone lengths even at maximum
    // breast pitch. A shorter, quicker step suits a long-legged ground bird.
    const CADENCE: f32 = 1.6;
    let cycle = w * CADENCE;
    let action = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.86, 1.0));
    let support = walk.max(action);
    let sway = cycle.sin() * 0.026 * walk;
    let rise = (cycle * 2.0).cos() * 0.012 * walk;
    let pitch = 0.045 * walk + breath * 0.012;
    p.o(
        J::Lift,
        Vec3::new(0.025 * cycle.sin() * walk, -0.09 * support + rise, 0.0),
    );
    p.r(J::Body, Quat::from_rotation_z(sway) * bow(pitch));
    // Carry the eye line while the breast and hips transfer weight beneath it.
    // The quick, purposeful looks in idle() remain the only head twitches.
    p.r(
        J::Neck,
        bow(-pitch * 0.8) * Quat::from_rotation_z(-sway * 0.8),
    );
    p.o(J::Neck, Vec3::Y * (-rise * 0.7));
    p.r(J::Head, bow(-pitch * 0.2));
    let mut kick = 0.0;
    if let Some(m) = m {
        let hit = m.impact();
        let gather = envelope(t, 0.0, hit * 0.55, hit - 0.1, hit);
        let strike = envelope(t, hit - 0.07, hit, hit + 0.06, 1.0);
        if m.offensive() {
            if matches!(m, Move::Heavy | Move::Shove | Move::Hook | Move::Bash) {
                // One foot bears the bird while the other drives out; no
                // simultaneous hop masquerading as a planted kick.
                kick = strike;
                p.o(
                    J::Lift,
                    Vec3::new(0.055 * strike, -0.025 * gather, 0.06 * strike),
                );
                p.r(J::Body, bow(-0.14 * strike + 0.08 * gather));
                p.r(J::Neck, bow(0.14 * strike));
                p.r(J::Head, bow(-0.12 * strike));
            } else {
                // Neck retracts first, then the beak leads the breast through.
                p.r(J::Neck, bow(-0.34 * gather + 0.95 * strike));
                p.r(J::Head, bow(0.18 * gather - 0.2 * strike));
                p.r(J::Body, bow(-0.08 * gather + 0.12 * strike));
                p.o(
                    J::Lift,
                    Vec3::new(0.0, -0.025 * gather, -0.06 * gather + 0.16 * strike),
                );
            }
        } else {
            p.r(J::Neck, bow(-0.2 * action));
            p.r(J::Body, bow(-0.07 * action));
            if m == Move::Step {
                p.o(J::Lift, Vec3::new(0.0, 0.0, -0.16 * action));
            }
        }
    }
    if !local_legs {
        return;
    }
    for (thigh, shin, foot, side, offset) in [
        (J::LegL, J::ShinL, J::FootL, 1.0, 0.0),
        (J::LegR, J::ShinR, J::FootR, -1.0, 0.5),
    ] {
        let (z, lift) = beast_step(cycle / TAU + offset, 0.6, 0.6 / (0.9 * CADENCE), 0.1);
        let raised = if side < 0.0 { kick } else { 0.0 };
        beast_limb(
            p,
            thigh,
            shin,
            foot,
            Vec3::Y,
            Vec3::new(side * 0.11, -0.1, 0.0),
            Vec3::new(0.0, -0.4, 0.08),
            BIRD_ANKLE,
            Vec3::new(
                side * 0.11,
                0.042 + lift * walk + 0.28 * raised,
                0.02 + z * walk + 0.38 * raised,
            ),
            1.0,
            support,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn quad_pose(
    p: &mut Pose,
    rig: &Rig,
    s: f32,
    w: f32,
    breath: f32,
    m: Option<Move>,
    t: f32,
    local_legs: bool,
) {
    let walk = ease(s * 4.0);
    let trot = ease((s - 0.38) / 0.45);
    let cycle = w * 1.35;
    let stance = 0.69 - 0.12 * trot;
    let span = stance / (0.9 * 1.35 * rig.stride.max(0.1));
    let action = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.86, 1.0));
    let support = walk.max(action);
    let roll = cycle.sin() * 0.022 * walk * (1.0 - 0.6 * trot);
    let pitch = (cycle * 2.0 - 0.4).sin() * 0.016 * walk + breath * 0.008;
    let rise = (cycle * 2.0).cos() * (0.008 + 0.01 * trot) * walk;
    // Idle geometry puts every sole on the floor. The lower travelling
    // pelvis leaves room for the knee to flex through a planted full stride.
    p.o(J::Lift, Vec3::Y * (-0.113 * rig.legs * support + rise));
    p.r(J::Body, bow(pitch) * Quat::from_rotation_z(roll));
    p.r(
        J::Neck,
        bow(-pitch * 0.75 + breath * 0.008) * Quat::from_rotation_z(-roll * 0.65),
    );
    p.r(J::Snout, bow(-breath * 0.008));
    // A loose tail follows the gait with a delay, rather than leading the body.
    p.r(
        J::Tail,
        Quat::from_rotation_y((cycle - 0.7).sin() * 0.12 * walk + breath * 0.035),
    );
    if rig.plan == Plan::Centaur {
        p.r(
            J::Torso,
            bow(-pitch * 0.85) * Quat::from_rotation_z(-roll * 0.85),
        );
        p.r(J::Head, bow(pitch * 0.2));
        p.r(
            J::ArmL,
            fwd(-cycle.sin() * 0.12 * walk) * Quat::from_rotation_z(0.08),
        );
        p.r(J::ArmR, fwd(0.35) * Quat::from_rotation_z(-0.08));
        p.r(J::ForeR, fwd(0.75));
    }
    let mut rear = 0.0;
    let mut stamp = 0.0;
    if rig.style.rogue_beast() {
        rogue::beast_pose(p, rig, breath, m, t, walk);
    } else if let Some(m) = m {
        let hit = m.impact();
        let gather = envelope(t, 0.0, hit * 0.55, hit - 0.1, hit);
        let strike = envelope(t, hit - 0.07, hit, hit + 0.06, 1.0);
        if !m.offensive() {
            p.r(J::Body, bow(-0.035 * action));
            p.r(J::Neck, bow(-0.14 * action));
            if m == Move::Step {
                p.o(J::Lift, Vec3::new(0.0, 0.0, -0.14 * action));
            }
            p.r(J::ArmL, fwd(0.65 * action));
        } else if rig.plan == Plan::Centaur {
            p.r(J::ArmR, fwd(-0.55 * gather + 0.9 * strike));
            p.r(J::ForeR, fwd(-0.5 * strike));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.27 * gather - 0.22 * strike) * bow(0.13 * strike),
            );
            p.r(J::ArmL, fwd(0.22 * gather - 0.12 * strike));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.025 * gather, -0.055 * gather + 0.16 * strike),
            );
        } else if matches!(m, Move::Heavy | Move::Bash | Move::Shove) {
            // Hoof strike: the haunches load before the forequarters rise.
            // Each front limb follows a deliberate arc, never a sine-wave paw.
            rear = gather;
            stamp = strike;
            p.r(J::Body, bow(-0.38 * rear + 0.07 * stamp));
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.1 * rear - 0.035 * stamp, -0.13 * rear + 0.1 * stamp),
            );
            p.r(J::Neck, bow(0.17 * rear - 0.06 * stamp));
        } else {
            // A unicorn aims its horn. Equines without a horn reach to bite;
            // neither needs to rear up for every ordinary attack.
            let horn = rig.style == Style::Unicorn;
            p.r(
                J::Neck,
                bow(-0.13 * gather + if horn { 0.54 } else { 0.29 } * strike),
            );
            p.r(J::Snout, bow(if horn { 0.19 } else { 0.06 } * strike));
            p.r(J::Body, bow(0.045 * strike));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.025 * gather, -0.075 * gather + 0.19 * strike),
            );
        }
    }
    if !local_legs {
        return;
    }
    for (thigh, shin, hoof, offset, front, side) in [
        (J::FL, J::FLs, J::HoofFL, 0.0, true, 1.0),
        (J::FR, J::FRs, J::HoofFR, 0.5, true, -1.0),
        // Touchdowns run LF, RH, RF, LH in the walk; the diagonal pairs
        // converge continuously as the gait becomes a collected trot.
        (J::BL, J::BLs, J::HoofBL, 0.25 + 0.25 * trot, false, 1.0),
        (J::BR, J::BRs, J::HoofBR, -0.25 + 0.25 * trot, false, -1.0),
    ] {
        let clearance = if rig.style == Style::Unicorn {
            0.11
        } else {
            0.075 * rig.legs
        };
        let (z, lift) = beast_step(cycle / TAU + offset, stance, span, clearance);
        let hip = rig.rest[thigh as usize];
        let lead = if front {
            rear * 0.24 + stamp * 0.08
        } else {
            0.0
        };
        let raised = if front { rear * 0.32 } else { 0.0 };
        beast_limb(
            p,
            thigh,
            shin,
            hoof,
            rig.rest[J::Body as usize],
            hip,
            rig.rest[shin as usize],
            rig.rest[hoof as usize],
            Vec3::new(
                hip.x + side * 0.012 * rear,
                // The swept horn wall sits below the shallow inner sole.
                rig.ankle_floor + lift * walk + raised * rig.legs,
                hip.z + rig.rest[hoof as usize].z + z * walk + lead,
            ),
            if front { 1.0 } else { -1.0 },
            support,
        );
    }
}

fn death_pose(p: &mut Pose, rig: &Rig, d: f32, seed: f32) {
    let side = if seed > 5.0 { 1.0 } else { -1.0 };
    // A little variety from the seed, so no two bodies lie quite alike.
    let v = |k: u32| rand(u64::from(seed.to_bits()), k) - 0.5;
    match rig.plan {
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
            // The horse is already on its side. Turn the human chest face-up
            // around its own long axis; an additional sideways torso roll
            // would drive its shoulders and head beneath the floor.
            p.r(J::Torso, Quat::from_rotation_y(side * FRAC_PI_2 * d));
            // The generated forearm's forward belly, smooth-union allowance,
            // and Storybook mesh scale bound its floor support. Derive the
            // shoulder-to-elbow drop from each rig's actual upper-arm length.
            let arm_support = (0.056 * 1.05 + 0.004 + 0.03 * 0.25) * 1.12 + 0.006;
            for (arm, fore, spread) in [(J::ArmL, J::ForeL, 0.55), (J::ArmR, J::ForeR, -0.55)] {
                // At the settled quarter-roll, body/torso x and arm z are
                // vertical. The body's small x-axis slump preserves them.
                let shoulder_height = 0.2
                    + side * (rig.rest[J::Body as usize].x + rig.rest[J::Torso as usize].x)
                    + rig.rest[arm as usize].z;
                let length = rig.rest[fore as usize].length().max(0.001);
                let drop = ((shoulder_height - arm_support) / length)
                    .clamp(-0.9, 0.9)
                    .asin();
                let upper = Quat::from_rotation_z(spread) * bow(drop);
                // Replace the living weapon aim, including its elbow bend.
                // Counter-rotate the forearm so it rests along the floor.
                p.rot[arm as usize] = p.rot[arm as usize].slerp(upper, d);
                p.rot[fore as usize] = p.rot[fore as usize].slerp(bow(-drop), d);
            }
            // The spear's attachment turns its shaft across the grip. Relax
            // the wrist by the same quarter-turn to lay it beside the hand.
            p.rot[J::HandR as usize] = p.rot[J::HandR as usize].slerp(fwd(FRAC_PI_2), d);
            p.r(
                J::Head,
                Quat::from_rotation_y(side * 1.2 * d) * bow(0.4 * d),
            );
        }
        Plan::Quadruped => {
            // Fallen on its side: the body lies at its own girth above the
            // floor, close beside the spot where it stood.
            let height = rig.rest[J::Body as usize].y;
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * height * 0.715 * d, rig.girth * d, 0.0);
            limp_legs(p, side, d);
            rogue::death(p, rig, d, side);
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
        Plan::Bird if rig.style.flies() => rogue::death(p, rig, d, side),
        Plan::Rooted => rogue::death(p, rig, d, side),
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

/// Clearance under a body band follows the same taper as its geometry.
/// The separate sculpted skull has a deeper jaw than a body band's ellipse.
fn snake_ground_height(index: usize, roll: f32) -> f32 {
    if index == 0 {
        0.076
    } else {
        let radius = snake_segment_radius(index);
        radius * (roll.sin().powi(2) + (0.75 * roll.cos()).powi(2)).sqrt() + 0.003
    }
}

/// Ground the settled crystal core using its generated shard sections. The
/// circumscribed ring is a conservative bound for each six-sided section;
/// unlike a sphere bound it stays close to the visibly supporting shard.
fn settled_ice_height(rotation: Quat) -> f32 {
    let mut bottom = -0.24f32;
    for k in 0..9 {
        let a = k as f32 * TAU / 9.0;
        let direction = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
        let axis = rotation * direction;
        let tip = 0.3 + rand(k as u64, 3) * 0.15;
        let width = 0.055 + 0.025 * rand(19, k);
        let radial_y = (1.0 - axis.y * axis.y).max(0.0).sqrt();
        for (along, radius) in [
            (0.06, width * 0.55),
            (0.06 + (tip - 0.06) * 0.68, width),
            (0.06 + (tip - 0.06) * 0.9, width * 0.3),
            (tip, 0.0),
        ] {
            bottom = bottom.min(axis.y * along - radius * radial_y);
        }
    }
    -bottom * 0.65 + 0.006
}

/// A flat spiral laid from the tail outward, its last few bands lifting the
/// head in an S over the coil. The rattle rises from the centre.
fn rattle_coil(n: usize) -> [Vec3; SNAKE] {
    let mut points = [Vec3::ZERO; SNAKE];
    let mut theta = 0.0f32;
    for j in 0..n {
        let i = n - 1 - j;
        let radius = 0.07 + 0.075 * theta / TAU;
        points[i] = Vec3::new(theta.cos() * radius, 0.0, theta.sin() * radius);
        theta += 0.068 / radius;
    }
    for (i, point) in points.iter_mut().enumerate().take(n) {
        let head = (1.0 - i as f32 / 5.0).max(0.0);
        let rattle = (i as f32 + 4.0 - n as f32).max(0.0);
        point.y = snake_ground_height(i, 0.0) + 0.3 * ease(head) + 0.05 * rattle;
    }
    if n > 1 {
        // Face the head forward and draw the coil back beneath it.
        let forward = (points[0] - points[1]).with_y(0.0).normalize_or(Vec3::Z);
        let turn = Quat::from_rotation_arc(forward, Vec3::Z);
        let head = turn * points[0].with_y(0.0);
        for point in points.iter_mut().take(n) {
            let y = point.y;
            *point = (turn * point.with_y(0.0) - head + Vec3::Z * 0.42).with_y(y);
        }
    }
    points
}

/// Snakes and floaters place their pieces along paths rather than joints.
fn place_segments(rig: &Rig, a: &Anim, fun: f32, joints: &mut Query<&mut Transform, Without<Rig>>) {
    let (m, t) = a
        .action
        .filter(|_| a.dead.is_none())
        .map_or((None, 0.0), |(m, t)| (Some(m), t));
    let d = ease(a.dead.unwrap_or(a.fallen));
    let alive = 1.0 - d;
    let calm = (1.0 - ease(a.speed * 4.0))
        * f32::from(u8::from(m.is_none() && a.dead.is_none() && !a.quiet));
    let (w, gag, _) = antic(a);
    let spring = if gag == 0 { w * fun * calm } else { 0.0 };
    let hit = m.map_or(0.45, Move::impact);
    let gather = m.map_or(0.0, |_| envelope(t, 0.0, hit * 0.55, hit - 0.1, hit));
    let strike = m
        .filter(|m| m.offensive())
        .map_or(0.0, |_| envelope(t, hit - 0.07, hit, hit + 0.06, 1.0));
    if rogue::place(rig, a, fun, joints) {
        return;
    }
    match rig.plan {
        Plan::Serpent => {
            let n = rig.segments.len().min(SNAKE);
            if n == 0 {
                return;
            }
            let wave = a.walk * 2.2 + a.seed;
            let amplitude = 0.18 + 0.4 * ease(a.speed * 3.0);
            let rear = every(a.time + a.seed * 3.7, 8.0, 3.0, 6.5) * calm;
            let raised = 0.24 * gather + 0.13 * strike + 0.19 * rear;
            let mut live = Vec3::new(
                0.018 * wave.sin() * a.speed,
                snake_ground_height(0, 0.0) + raised,
                0.6 - 0.09 * gather + 0.24 * strike,
            );
            let mut settled = Vec3::new(0.0, snake_ground_height(0, 2.6), 0.6);
            let mut points = [Vec3::ZERO; SNAKE];
            // A rattlesnake rests coiled, head raised over its rattle.
            let rattler = rig.style == Style::Rattlesnake;
            let still = (1.0 - ease(a.speed * 4.0)) * f32::from(u8::from(m.is_none()));
            let coiled = if rattler { still * 0.92 } else { 0.0 };
            let coil = rattle_coil(n);
            let mut previous_y = live.y;
            for (i, point) in points.iter_mut().enumerate().take(n) {
                let f = i as f32;
                if i > 0 {
                    let front = (1.0 - f / 7.0).max(0.0);
                    let idle = (a.time * 0.9 + a.seed - f * 0.24).sin() * 0.003 * calm;
                    let y = snake_ground_height(i, 0.0) + raised * ease(front) + idle;
                    let dy = (y - previous_y).clamp(-0.06, 0.06);
                    let yaw = (wave - f * 0.42).sin() * amplitude * (0.75 + 0.25 * f / n as f32);
                    // Advancing a constant length along the tangent keeps the
                    // painted bands touching through a turn or raised strike.
                    let horizontal = (0.068f32.powi(2) - dy * dy).sqrt();
                    live += Vec3::new(-yaw.sin() * horizontal, dy, -yaw.cos() * horizontal);
                    previous_y = live.y;
                    let fall_yaw = (f * 0.38 + a.seed * 0.2).sin() * 0.48;
                    let fall_dy = snake_ground_height(i, 2.6) - settled.y;
                    let fall_span = (0.068f32.powi(2) - fall_dy * fall_dy).max(0.0).sqrt();
                    settled += Vec3::new(
                        -fall_yaw.sin() * fall_span,
                        fall_dy,
                        -fall_yaw.cos() * fall_span,
                    );
                }
                let mut at = live.lerp(coil[i], coiled).lerp(settled, d);
                if spring > 0.0 {
                    // The optional long-idle gag winds into a continuous helix,
                    // with approximately the same spacing as the resting body.
                    let turn = f * 0.48;
                    let bounce = (a.time * 7.0).sin().abs() * 0.1;
                    let coil = Vec3::new(
                        turn.cos() * 0.13,
                        0.46 - f * 0.02 + bounce * (1.0 - f / n as f32),
                        turn.sin() * 0.13,
                    );
                    at = at.lerp(coil, spring);
                }
                *point = at;
            }
            for (i, e) in rig.segments.iter().take(n).enumerate() {
                let before = points[i.saturating_sub(1)];
                let after = points[(i + 1).min(n - 1)];
                let tangent = before - after;
                // The raised neck supports a forward-looking skull rather
                // than pointing its eyes and fangs up at the ceiling.
                let direction = if i == 0 {
                    tangent.with_y(tangent.y * d).normalize_or(Vec3::Z)
                } else {
                    tangent.normalize_or(Vec3::Z)
                };
                // The rattle buzzes as a warning, at rest and when it strikes.
                let shake = if rattler && i + 3 >= n {
                    let buzz = ((calm * 0.6 + gather) * alive).min(1.0);
                    Quat::from_rotation_y((a.time * 42.0 + i as f32).sin() * 0.35 * buzz)
                } else {
                    Quat::IDENTITY
                };
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = points[i];
                    transform.rotation = Quat::from_rotation_arc(Vec3::Z, direction)
                        * Quat::from_rotation_z(d * 2.6)
                        * shake;
                }
            }
            if let Some(tongue) = rig.joint(J::Snout) {
                if let Ok(mut transform) = joints.get_mut(tongue) {
                    let cycle = (a.time * 0.45 + a.seed).rem_euclid(1.0);
                    let flick = if cycle < 0.12 {
                        (cycle / 0.12 * TAU).sin().powi(2)
                    } else {
                        0.0
                    };
                    let out = flick.max(strike * 0.7) * alive;
                    transform.scale = Vec3::new(1.0, 1.0, out.max(0.01));
                }
            }
        }
        Plan::Floater => {
            let ice = rig.style == Style::Ice;
            let phase = a.time + a.seed * 2.7;
            let hover = if ice {
                (phase * 1.3).sin() * 0.025
            } else {
                (phase * 1.8).sin() * 0.055
            };
            let flare = m.map_or(0.0, |_| envelope(t, 0.0, hit * 0.8, hit + 0.04, 1.0));
            let live_core = Vec3::new(
                (phase * 0.55).sin() * 0.015 * (1.0 - a.speed),
                1.1 + hover - 0.035 * gather,
                -0.04 * a.speed - 0.065 * gather + 0.25 * strike,
            );
            let fallen_rotation =
                Quat::from_rotation_y(a.seed * 0.4) * Quat::from_rotation_z(0.9) * bow(0.2);
            let fallen = Vec3::new(
                0.0,
                if ice {
                    settled_ice_height(fallen_rotation)
                } else {
                    0.055
                },
                0.0,
            );
            if let Some(core) = rig.joint(J::Core) {
                if let Ok(mut transform) = joints.get_mut(core) {
                    transform.translation = live_core.lerp(fallen, d);
                    // Ice keeps its mass and a readable face; fire breathes
                    // upward. Neither spins its eyes away from its target.
                    let pulse = if ice {
                        0.0
                    } else {
                        (phase * 3.6).sin() * 0.025
                    };
                    let charge = 1.0 + if ice { 0.08 } else { 0.16 } * flare;
                    let live_scale =
                        Vec3::new(1.0 - pulse * 0.5, 1.0 + pulse, 1.0 - pulse * 0.5) * charge;
                    transform.scale =
                        live_scale.lerp(Vec3::splat(if ice { 0.65 } else { 0.06 }), d);
                    let live_rotation = Quat::from_rotation_y((phase * 0.6).sin() * 0.08)
                        * bow(0.1 * a.speed - 0.14 * gather + 0.18 * strike)
                        * Quat::from_rotation_z(
                            (phase * 0.8).sin() * if ice { 0.035 } else { 0.06 },
                        );
                    transform.rotation = live_rotation.slerp(fallen_rotation, d);
                }
            }
            for (j, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
                if let Some(e) = rig.joint(j) {
                    if let Ok(mut transform) = joints.get_mut(e) {
                        let lag = (phase * 1.3 - 0.55 + side * 0.25).sin() * 0.035;
                        let live_rotation = Quat::from_rotation_y(-side * 0.22 * strike)
                            * Quat::from_rotation_z(side * (lag + 0.28 * flare));
                        // The broken shard fans lie out to either side of
                        // the core, supported at their broad roots. Cancel
                        // the core's fall rotation so neither arm is buried.
                        let ground_rotation =
                            fallen_rotation.inverse() * Quat::from_rotation_z(side * FRAC_PI_2);
                        let ground_at = fallen_rotation.inverse()
                            * Vec3::new(side * 0.42, (0.025 - fallen.y) / 0.65, 0.06);
                        transform.translation = rig.rest[j as usize].lerp(ground_at, d);
                        transform.rotation = live_rotation.slerp(ground_rotation, d);
                    }
                }
            }
            let count = rig.segments.len().max(1) as f32;
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                let angle = phase * if ice { 0.55 } else { 0.9 } + f * TAU / count;
                let (at, size) = if ice {
                    let radius = 0.32 + 0.14 * flare;
                    (
                        live_core
                            + Vec3::new(
                                angle.cos() * radius,
                                angle.sin() * 0.13,
                                angle.sin() * radius,
                            ),
                        0.8,
                    )
                } else {
                    // Embers rise in a loose column and fade at both ends of
                    // their life, so recycling has no visible positional pop.
                    let life = (phase * 0.28 + f / count).rem_euclid(1.0);
                    let radius = 0.13 + life * 0.16 + flare * 0.1;
                    (
                        live_core
                            + Vec3::new(
                                angle.cos() * radius,
                                -0.1 + life * 0.65,
                                angle.sin() * radius,
                            ),
                        (PI * life).sin().powi(2),
                    )
                };
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = at.lerp(fallen, d);
                    transform.scale = Vec3::splat(size * alive);
                }
            }
        }
        _ => {}
    }
}
