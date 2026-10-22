//! Motion for the Rogue scenario's own monsters: fliers on the wing, the
//! rooted flytrap, beasts' jaws and wings, and each kind's fidgets and fun.
use super::*;

fn roll(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

fn turn(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}

/// Gather and strike envelopes shared by every offensive move.
fn timing(m: Move, t: f32) -> (f32, f32, f32) {
    let hit = m.impact();
    let gather = envelope(t, 0.0, hit * 0.55, hit - 0.1, hit);
    let strike = envelope(t, hit - 0.07, hit, hit + 0.06, 1.0);
    let act = envelope(t, 0.0, 0.12, 0.86, 1.0);
    (gather, strike, act)
}

/// Raise folded wings into a mantle: the leading edge swings out level while
/// the hand keeps the trailing skin or feathers hanging behind it.
fn mantle(p: &mut Pose, k: f32) {
    p.r(J::WingL, roll(-1.15 * k) * fwd(0.25 * k));
    p.r(J::WingR, roll(1.15 * k) * fwd(0.25 * k));
    p.r(J::WingTipL, roll(0.8 * k) * turn(0.5 * k));
    p.r(J::WingTipR, roll(-0.8 * k) * turn(-0.5 * k));
}

// ---------------------------------------------------------------------------
// Fliers.

pub(super) fn flier_pose(
    p: &mut Pose,
    rig: &Rig,
    a: &Anim,
    s: f32,
    breath: f32,
    m: Option<Move>,
    t: f32,
) {
    let bat = rig.style == Style::Bat;
    let alive = if a.dead.is_none() { 1.0 } else { 0.0 };
    let time = a.time + a.seed;
    let rate = if bat { 3.4 } else { 2.6 };
    let phase = time * TAU * rate;
    // A kestrel hangs on the wind in bursts of beats, then glides; a bat
    // never stops beating and jinks from side to side.
    let glide = if bat { 0.0 } else { every(time, 6.0, 3.4, 5.8) };
    let amp = (if bat { 0.8 } else { 0.62 }) * (1.0 - 0.8 * glide) * alive;
    let beat = phase.sin() * amp;
    let dihedral = 0.18 * glide * alive;
    let jink = if bat { (time * 1.7).sin() * 0.07 } else { 0.0 };
    p.o(
        J::Lift,
        Vec3::new(
            jink * alive,
            (-0.05 * phase.cos() * amp + 0.04 * (time * 0.8).sin()) * alive,
            0.0,
        ),
    );
    p.r(J::Lift, roll(-jink * 1.5 * alive));
    p.r(
        J::Body,
        bow((0.3 * s + 0.06 * beat + breath * 0.01) * alive),
    );
    let sweep = 0.22 * phase.cos() * amp;
    p.r(J::WingL, roll(beat + dihedral) * turn(-sweep));
    p.r(J::WingR, roll(-beat - dihedral) * turn(sweep));
    let lag = (phase - 0.7).sin() * amp * 0.55;
    p.r(J::WingTipL, roll(lag));
    p.r(J::WingTipR, roll(-lag));
    // The head stays level while the body rises and falls beneath it.
    p.r(J::Neck, bow((-0.3 * s - 0.06 * beat) * alive));
    p.r(J::Tail, bow(-0.12 * beat) * roll(0.1 * jink));
    for leg in [J::LegL, J::LegR] {
        p.r(leg, fwd(-0.9 * alive));
    }
    let Some(m) = m else {
        return;
    };
    let (gather, strike, act) = timing(m, t);
    if m.offensive() {
        // Climb with wings high, then stoop on the target, talons forward.
        p.o(
            J::Lift,
            Vec3::new(
                0.0,
                0.14 * gather - 0.34 * strike,
                -0.12 * gather + 0.42 * strike,
            ),
        );
        p.r(J::Body, bow(-0.35 * gather + 0.55 * strike));
        p.r(J::WingL, roll(0.9 * gather - 0.3 * strike));
        p.r(J::WingR, roll(-0.9 * gather + 0.3 * strike));
        for (leg, shin) in [(J::LegL, J::ShinL), (J::LegR, J::ShinR)] {
            p.r(leg, fwd(1.9 * strike));
            p.r(shin, fwd(0.5 * strike));
        }
        p.r(J::Head, bow(0.25 * strike));
    } else {
        // Flare: wings wide, body upright, hanging in the air.
        p.r(J::Body, bow(-0.4 * act));
        p.r(J::WingL, roll(0.35 * act));
        p.r(J::WingR, roll(-0.35 * act));
        p.o(J::Lift, Vec3::Y * 0.1 * act);
    }
}

// ---------------------------------------------------------------------------
// The flytrap.

pub(super) fn rooted_pose(p: &mut Pose, a: &Anim, breath: f32, m: Option<Move>, t: f32) {
    let alive = if a.dead.is_none() { 1.0 } else { 0.0 };
    let time = a.time + a.seed * 3.7;
    let sway = (time * 0.7).sin() * alive;
    p.r(J::Neck, roll(sway * 0.07) * bow(0.04 * breath));
    p.r(
        J::Torso,
        roll(-sway * 0.06) * bow((0.12 + 0.04 * (time * 0.9).sin()) * alive),
    );
    // At rest the trap hangs a little open, breathing.
    let mut gape = (0.55 + 0.10 * (time * 0.8).sin()) * alive;
    if let Some(m) = m {
        let (gather, strike, act) = timing(m, t);
        if m.offensive() {
            // Draw back gaping, then lunge and snap shut.
            p.r(J::Neck, bow(-0.4 * gather + 0.6 * strike));
            p.r(J::Torso, bow(-0.25 * gather + 0.55 * strike));
            gape = (gape + 0.6 * gather) * (1.0 - strike);
        } else {
            gape = gape.lerp(0.9, act);
        }
    }
    let gape = gape.clamp(0.0, 1.0);
    p.r(J::Head, bow(-0.55 * gape));
    p.r(J::Jaw, bow(1.5 * gape));
}

// ---------------------------------------------------------------------------
// Beasts: jaws, wings and their signature moves.

pub(super) fn beast_pose(
    p: &mut Pose,
    rig: &Rig,
    breath: f32,
    m: Option<Move>,
    t: f32,
    walk: f32,
    cycle: f32,
) {
    let style = rig.style;
    let winged = matches!(style, Style::Griffin | Style::Dragon);
    // Jaws rest a fraction open and breathe; folded wings settle as it walks.
    let rest_jaw = match style {
        Style::Xeroc => 0.06,
        Style::Aquator => 0.03,
        _ => 0.08,
    };
    p.r(J::Jaw, bow(rest_jaw + 0.03 * breath));
    if winged {
        let shuffle = 0.04 * breath + 0.06 * walk;
        mantle(p, shuffle);
    }
    if style == Style::Jabberwock {
        // It carries its chest high and its head forward.
        p.r(J::Neck, bow(0.1));
    }
    grasping_forelimbs(p, rig, m, t, walk, cycle);
    let Some(m) = m else {
        return;
    };
    let (gather, strike, act) = timing(m, t);
    match m {
        Move::Breath(_) => {
            // Rear back drawing breath, then thrust the head forward, jaw wide.
            let pour = envelope(t, m.impact() - 0.08, m.impact(), 0.85, 1.0);
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.08 * gather, -0.12 * gather + 0.1 * pour),
            );
            p.r(J::Body, bow(-0.18 * gather + 0.06 * pour));
            p.r(J::Neck, bow(-0.55 * gather + 0.4 * pour));
            p.r(J::Head, bow(0.3 * gather - 0.25 * pour));
            p.r(J::Jaw, bow(0.25 * gather + 0.85 * pour));
            if winged {
                mantle(p, 1.1 * act);
            }
        }
        Move::Fear => {
            // A roar, wings raised.
            p.r(J::Neck, bow(-0.45 * act));
            p.r(J::Jaw, bow(0.85 * act));
            p.o(J::Lift, Vec3::new(0.0, 0.05 * act, -0.06 * act));
            if winged {
                mantle(p, 1.2 * act);
            }
        }
        Move::Shove => {
            // An ambush: crouch, gape, then a lunge that clamps shut.
            p.o(
                J::Lift,
                Vec3::new(
                    0.0,
                    -0.05 * gather * rig.legs,
                    -0.1 * gather + 0.42 * strike,
                ),
            );
            p.r(J::Body, bow(0.1 * gather - 0.08 * strike));
            p.r(J::Head, bow(-0.25 * gather + 0.15 * strike));
            p.r(J::Jaw, bow(0.95 * gather * (1.0 - strike)));
        }
        Move::Touch(_) => {
            // The aquator's contact sweep: low and forward, tendrils leading.
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.04 * gather, -0.06 * gather + 0.22 * strike),
            );
            p.r(J::Neck, bow(0.25 * strike));
            p.r(J::Body, roll(0.12 * strike));
        }
        _ if m.offensive() => {
            // A bite: the neck coils back, then strikes, jaws snapping shut.
            p.r(J::Neck, bow(-0.25 * gather + 0.5 * strike));
            p.r(J::Head, bow(0.15 * gather - 0.15 * strike));
            p.r(J::Jaw, bow(0.75 * gather * (1.0 - strike)));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.02 * gather, -0.07 * gather + 0.2 * strike),
            );
            if winged {
                mantle(p, 0.5 * act);
            }
        }
        _ => {
            p.r(J::Neck, bow(-0.15 * act));
            p.r(J::Body, bow(-0.04 * act));
            if m == Move::Step {
                p.o(J::Lift, Vec3::new(0.0, 0.0, -0.14 * act));
            }
            if winged {
                mantle(p, 0.4 * act);
            }
        }
    }
}

/// Free forelimbs balance the stride and reach into a two-handed rake. This
/// follows limb roles, so another reared body can use the same motion without
/// inheriting Jabberwock's neck, jaw, or tail recipe.
fn grasping_forelimbs(p: &mut Pose, rig: &Rig, m: Option<Move>, t: f32, walk: f32, cycle: f32) {
    let (gather, strike, act) = m.map_or((0.0, 0.0, 0.0), |m| timing(m, t));
    let attacking = m.is_some_and(Move::offensive);
    for (upper, fore, hand, side, phase) in [
        (J::FL, J::FLs, J::HoofFL, 1.0, 0.0),
        (J::FR, J::FRs, J::HoofFR, -1.0, PI),
    ] {
        if rig.supports(hand) {
            continue;
        }
        let swing = (cycle + phase).sin() * 0.09 * walk;
        p.r(upper, fwd(swing) * roll(side * 0.055));
        p.r(fore, fwd(-swing * 0.6));
        if attacking {
            p.r(
                upper,
                fwd(0.55 * gather + 1.0 * strike) * roll(side * (0.24 * gather - 0.08 * strike)),
            );
            p.r(fore, fwd(-0.35 * gather + 0.35 * strike));
            p.r(hand, fwd(-0.22 * strike));
        } else if m.is_some() {
            p.r(upper, fwd(0.35 * act) * roll(side * 0.18 * act));
            p.r(fore, fwd(0.2 * act));
        }
    }
}

/// The dead: slack jaws and wings, fallen fliers, a wilted flytrap.
pub(super) fn death(p: &mut Pose, rig: &Rig, d: f32, side: f32) {
    match rig.plan {
        Plan::Bird => {
            // Dropped from the air onto its side, wings askew.
            let height = rig.rest[J::Body as usize].y - rig.girth;
            p.rot[J::Lift as usize] =
                Quat::from_rotation_z(side * 1.3 * d) * Quat::from_rotation_x(0.2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * (height + 0.1) * d, -height * 0.02 * d, 0.0);
            p.r(J::WingL, roll(-0.5 * d));
            p.r(J::WingR, roll(0.3 * d));
            p.r(J::Neck, bow(0.6 * d) * turn(side * 0.5 * d));
            p.r(J::Head, bow(0.3 * d));
            for leg in [J::LegL, J::LegR] {
                p.r(leg, fwd(0.6 * d));
            }
        }
        Plan::Rooted => {
            // Wilted over: the stem bends to the floor and the trap gapes.
            p.r(J::Neck, bow(0.75 * d) * roll(side * 0.25 * d));
            p.r(J::Torso, bow(0.95 * d));
            p.r(J::Head, bow(0.35 * d) * turn(side * 0.4 * d));
            p.r(J::Jaw, bow(0.55 * d));
        }
        _ => {
            p.r(J::Jaw, bow(0.4 * d));
            p.r(J::Tail, turn(-side * 0.5 * d));
            if matches!(rig.style, Style::Griffin | Style::Dragon) {
                // The upper wing falls open across the flank.
                let (upper, tip) = if side > 0.0 {
                    (J::WingR, J::WingTipR)
                } else {
                    (J::WingL, J::WingTipL)
                };
                p.r(upper, roll(side * 0.6 * d));
                p.r(tip, roll(-side * 0.4 * d));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Humanoid moves that only Rogue monsters use.

pub(super) fn biped_move(p: &mut Pose, m: Move, t: f32) {
    let (gather, strike, act) = timing(m, t);
    match m {
        Move::Gaze => {
            // A level, unblinking stare, clawed hands spread low.
            let k = act;
            p.r(J::Head, bow(-0.12 * k) * roll((t * 30.0).sin() * 0.015 * k));
            p.r(J::Torso, bow(0.12 * k));
            p.r(J::ArmL, fwd(0.5 * k) * roll(0.65 * k));
            p.r(J::ArmR, fwd(0.5 * k) * roll(-0.65 * k));
            p.r(J::ForeL, fwd(0.7 * k));
            p.r(J::ForeR, fwd(0.7 * k));
            p.o(J::Lift, Vec3::new(0.0, -0.03 * k, 0.06 * k));
        }
        Move::Bite(_) => {
            p.r(J::Torso, bow(-0.1 * gather + 0.4 * strike));
            p.r(J::Head, bow(0.2 * strike));
            p.r(J::ArmL, fwd(0.4 * gather + 1.1 * strike));
            p.r(J::ArmR, fwd(0.4 * gather + 1.1 * strike));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.04 * gather, -0.06 * gather + 0.3 * strike),
            );
        }
        _ => {
            // A breath from a person: chest swelling, then a forward exhalation.
            p.r(J::Torso, bow(-0.2 * gather + 0.3 * strike));
            p.r(J::Head, bow(-0.25 * gather + 0.2 * strike));
            p.r(J::ArmL, roll(0.5 * gather));
            p.r(J::ArmR, roll(-0.5 * gather));
            p.o(J::Lift, Vec3::new(0.0, 0.0, 0.15 * strike));
        }
    }
}

// ---------------------------------------------------------------------------
// Idle fidgets: true when this replaces the shared idle; people add their
// fidget on top of it.

pub(super) fn idle(p: &mut Pose, rig: &Rig, time: f32, seed: f32, k: f32) -> bool {
    let look = (time * 0.37).sin() * 0.3 + (time * 0.13).sin() * 0.2;
    match rig.style {
        Style::Orc => {
            // Cracks its knuckles.
            let f = every(time, 7.0, 4.0, 6.0) * k;
            let crack = (time * 11.0).sin() * 0.06 * f;
            p.r(J::ArmL, fwd(0.95 * f) * roll(-0.35 * f));
            p.r(J::ArmR, fwd(0.95 * f) * roll(0.35 * f));
            p.r(J::ForeL, fwd((1.4 + crack) * f));
            p.r(J::ForeR, fwd((1.4 - crack) * f));
            p.r(J::Head, bow(0.12 * f));
            false
        }
        Style::Troll => {
            // A heavy sway, and a scratch of the scalp.
            p.r(J::Lift, roll((time * 0.5).sin() * 0.05 * k));
            let f = every(time, 9.0, 5.0, 7.5) * k;
            p.r(J::ArmR, roll(-1.6 * f) * fwd(0.4 * f));
            p.r(J::ForeR, fwd((2.0 + (time * 12.0).sin() * 0.15) * f));
            p.r(J::Head, roll(0.25 * f));
            false
        }
        Style::Leprechaun => {
            // Bounces on its toes and flips a coin.
            p.o(J::Lift, Vec3::Y * (time * 3.0).sin().abs() * 0.018 * k);
            let f = every(time, 6.0, 3.0, 4.6) * k;
            p.r(J::ArmR, fwd(0.6 * f));
            p.r(J::ForeR, fwd(0.9 * f));
            p.r(J::HandR, Quat::from_rotation_x(f * TAU * 2.0));
            p.r(J::Head, bow(-0.3 * f));
            false
        }
        Style::Nymph => {
            // Sways gracefully and opens a hand in welcome.
            p.r(J::Torso, roll((time * 0.8).sin() * 0.05 * k));
            let f = every(time, 8.0, 4.0, 7.0) * k;
            p.r(J::ArmR, roll(-0.9 * f) * fwd(0.45 * f));
            p.r(J::ForeR, fwd(0.25 * f));
            p.r(J::Head, roll(0.15 * f) * turn(-0.2 * f));
            false
        }
        Style::Medusa => {
            // Hips sway; now and then she flexes her claws.
            p.r(J::Hips, roll((time * 0.7).sin() * 0.06 * k));
            p.r(J::Head, roll((time * 0.4).sin() * 0.1 * k));
            let f = every(time, 7.5, 4.0, 6.0) * k;
            p.r(J::ArmL, fwd(0.7 * f) * roll(0.3 * f));
            p.r(J::ForeL, fwd((1.0 + (time * 8.0).sin() * 0.12) * f));
            false
        }
        Style::Phantom => {
            // Hangs just off the floor, arms drifting.
            p.o(J::Lift, Vec3::Y * (0.03 + (time * 1.1).sin() * 0.02) * k);
            p.r(J::ArmL, fwd((0.2 + (time * 0.9).sin() * 0.1) * k));
            p.r(J::ArmR, fwd((0.2 + (time * 0.9 + 1.0).sin() * 0.1) * k));
            false
        }
        Style::Wraith => {
            // Hunched and watchful; a slow reach of one claw.
            p.r(J::Torso, bow(0.08 * k));
            let f = every(time, 9.0, 5.0, 8.0) * k;
            p.r(J::ArmL, fwd(1.2 * f) * roll(0.2 * f));
            p.r(J::ForeL, fwd(0.15 * f));
            p.r(J::Head, bow(-0.15 * f));
            false
        }
        Style::Bat | Style::Kestrel => {
            // Glances about; a kestrel cocks its head.
            p.r(J::Neck, turn(look * 1.2 * k));
            if rig.style == Style::Kestrel {
                let f = every(time, 5.0, 2.0, 3.0) * k;
                p.r(J::Head, roll(0.4 * f));
            }
            true
        }
        Style::Flytrap => {
            p.r(J::Torso, turn(look * 0.6 * k));
            true
        }
        Style::Griffin | Style::Dragon | Style::Jabberwock | Style::Aquator | Style::Xeroc => {
            let sway = (time * 0.6).sin();
            p.r(J::Tail, turn((time * 1.4).sin() * 0.3 * k));
            p.r(J::Body, roll(sway * 0.02 * k));
            p.r(J::Neck, turn(look * 0.5 * k));
            match rig.style {
                Style::Dragon => {
                    // Lifts its head to snort smoke and resettles its wings.
                    let f = every(time, 7.0, 3.5, 5.5) * k;
                    p.r(J::Neck, bow(-0.25 * f));
                    p.r(J::Jaw, bow(0.25 * f));
                    mantle(p, 0.15 * every(time + seed, 9.0, 1.0, 2.2) * k);
                }
                Style::Griffin => {
                    // Preens under one wing.
                    let f = every(time, 9.0, 4.5, 7.5) * k;
                    p.r(J::Neck, turn(1.1 * f) * bow(0.55 * f));
                    p.r(J::Head, bow(0.35 * f) * roll((time * 9.0).sin() * 0.1 * f));
                    p.r(J::WingL, roll(-0.3 * f));
                }
                Style::Jabberwock => {
                    // Jerks its head about and chatters its jaw.
                    let jerk = ((time * 1.1).sin() * 4.0).round() * 0.1;
                    p.r(J::Neck, turn(jerk * k));
                    let f = every(time, 5.0, 2.5, 3.6) * k;
                    p.r(J::Jaw, bow((0.35 + (time * 22.0).sin() * 0.25) * f));
                }
                Style::Aquator => {
                    // Gulps, its throat working.
                    let f = every(time, 6.0, 3.0, 3.8) * k;
                    p.r(J::Jaw, bow(0.35 * f));
                    p.r(J::Head, bow(-0.12 * f));
                }
                _ => {
                    // Lies in wait, its mouth slowly widening.
                    let f = every(time, 8.0, 3.0, 6.5) * k;
                    p.r(J::Jaw, bow(0.45 * f));
                }
            }
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Antics while the player is away: true when handled here.

pub(super) fn antics(p: &mut Pose, rig: &Rig, time: f32, gag: u32, progress: f32, k: f32) -> bool {
    let beat = time * TAU * 1.6;
    match (rig.style, gag % 2) {
        (Style::Orc, 0) => {
            // Flexes both arms, grinning at the room.
            let pump = (time * 3.0).sin().abs();
            p.r(J::ArmL, roll((1.5 + 0.1 * pump) * k));
            p.r(J::ArmR, roll(-(1.5 + 0.1 * pump) * k));
            p.r(J::ForeL, fwd((1.7 + 0.3 * pump) * k));
            p.r(J::ForeR, fwd((1.7 + 0.3 * pump) * k));
            p.r(J::Torso, bow(-0.15 * k));
            p.o(J::Lift, Vec3::Y * 0.02 * pump * k);
        }
        (Style::Troll, 0) => {
            // Drums on its belly.
            let drum = (time * 10.0).sin();
            p.r(J::ArmL, fwd((0.6 + 0.2 * drum) * k));
            p.r(J::ArmR, fwd((0.6 - 0.2 * drum) * k));
            p.r(J::ForeL, fwd(1.3 * k));
            p.r(J::ForeR, fwd(1.3 * k));
            p.r(J::Head, bow(-0.35 * k));
            p.r(J::Torso, bow(-0.15 * k));
        }
        (Style::Leprechaun, 0) => {
            // A quick jig with heel clicks in the air.
            let b = beat.sin();
            let hop = (beat * 2.0).sin().abs();
            p.o(J::Lift, Vec3::Y * 0.12 * hop * k);
            p.r(J::LegL, fwd(0.6 * b.max(0.0) * k) * roll(-0.25 * hop * k));
            p.r(J::LegR, fwd(0.6 * (-b).max(0.0) * k) * roll(0.25 * hop * k));
            p.r(J::ShinL, fwd(-0.9 * hop * k));
            p.r(J::ShinR, fwd(-0.9 * hop * k));
            p.r(J::ArmL, roll(0.3 * k) * fwd(0.4 * k));
            p.r(J::ArmR, roll(-0.3 * k) * fwd(0.4 * k));
            p.r(J::ForeL, fwd(1.5 * k));
            p.r(J::ForeR, fwd(1.5 * k));
        }
        (Style::Leprechaun, _) => {
            // Spins so fast it all but vanishes, then bows.
            p.r(J::Lift, turn(spins(progress, 8.0)));
            p.r(J::ArmL, roll(1.2 * k));
            p.r(J::ArmR, roll(-1.2 * k));
            let bow_out = ease((progress - 0.85) / 0.15) * k;
            p.r(J::Torso, bow(0.6 * bow_out));
        }
        (Style::Nymph, 0) => {
            // A turning dance, arms raised.
            p.r(J::Lift, turn(spins(progress, 3.0)));
            p.o(J::Lift, Vec3::Y * 0.04 * beat.sin().abs() * k);
            p.r(J::ArmL, roll((2.4 + 0.2 * beat.sin()) * k));
            p.r(J::ArmR, roll(-(2.0 - 0.2 * beat.sin()) * k));
            p.r(J::ForeL, roll(-0.4 * k));
            p.r(J::ForeR, roll(0.4 * k));
            p.r(J::LegL, fwd(0.3 * k));
            p.r(J::Head, bow(-0.25 * k));
        }
        (Style::Medusa, 0) => {
            // A charmer's sway, arms undulating like her serpents.
            let w = (time * 2.0).sin();
            p.r(J::Hips, roll(0.12 * w * k));
            p.r(J::Torso, roll(-0.14 * w * k));
            p.r(J::ArmL, roll((1.3 + 0.3 * w) * k));
            p.r(J::ArmR, roll(-(1.3 - 0.3 * w) * k));
            p.r(J::ForeL, roll(-0.6 * w * k));
            p.r(J::ForeR, roll(-0.6 * w * k));
            p.r(J::Head, roll(0.15 * w * k));
        }
        (Style::Phantom, 0) | (Style::Wraith, 0) => {
            // Rises and drifts round in a slow circle.
            p.r(J::Lift, turn(spins(progress, 1.0)));
            p.o(J::Lift, Vec3::Y * (0.32 + (time * 1.3).sin() * 0.05) * k);
            p.r(J::ArmL, roll(0.9 * k) * fwd(0.3 * k));
            p.r(J::ArmR, roll(-0.9 * k) * fwd(0.3 * k));
            for (leg, shin) in [(J::LegL, J::ShinL), (J::LegR, J::ShinR)] {
                p.r(leg, fwd(0.3 * k));
                p.r(shin, fwd(-0.5 * k));
            }
        }
        (Style::Phantom, _) | (Style::Wraith, _) => {
            // Boo: arms up, looming forward.
            let loom = (time * 1.5).sin().abs();
            p.r(J::ArmL, fwd((2.4 + 0.2 * loom) * k));
            p.r(J::ArmR, fwd((2.4 + 0.2 * loom) * k));
            p.r(J::ForeL, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.6 * k));
            p.r(J::Torso, bow(0.3 * loom * k));
            p.o(J::Lift, Vec3::new(0.0, 0.08 * k, 0.15 * loom * k));
        }
        (Style::Orc | Style::Troll | Style::Nymph | Style::Medusa, _) => {
            if rig.style == Style::Medusa {
                // Tosses her serpent hair, hand on hip.
                p.r(J::Head, roll((time * 3.0).sin() * 0.35 * k) * bow(-0.2 * k));
                p.r(J::ArmL, roll(0.6 * k) * fwd(-0.3 * k));
                p.r(J::ForeL, fwd(1.6 * k));
            } else {
                stretch(p, time, k);
            }
        }
        (Style::Bat | Style::Kestrel, 0) => {
            // Loops the loop.
            let a = spins(progress, 1.0);
            p.o(
                J::Lift,
                Vec3::new(0.0, (1.0 - a.cos()) * 0.4, a.sin() * 0.4),
            );
            p.r(J::Lift, Quat::from_rotation_x(-a));
        }
        (Style::Bat | Style::Kestrel, _) => {
            // A barrel roll, then another.
            p.r(J::Lift, Quat::from_rotation_z(spins(progress, 2.0)));
            p.o(J::Lift, Vec3::Y * 0.2 * k);
        }
        (Style::Flytrap, 0) => {
            // Dances on its stem.
            let w = beat.sin();
            p.r(J::Neck, roll(0.35 * w * k));
            p.r(J::Torso, roll(-0.5 * w * k) * bow(0.15 * k));
            p.r(J::Head, roll(0.3 * w * k));
        }
        (Style::Flytrap, _) => {
            // Snaps at a fly only it can see.
            let snap = (time * 6.0).sin().max(0.0);
            p.r(
                J::Torso,
                turn((time * 1.7).sin() * 0.6 * k) * bow(0.3 * snap * k),
            );
            p.r(J::Head, bow(-0.5 * (1.0 - snap) * k));
            p.r(J::Jaw, bow(0.7 * (1.0 - snap) * k));
        }
        (Style::Dragon | Style::Griffin, 0) => {
            // Rears, beating its wings.
            p.r(J::Lift, Quat::from_rotation_x(-0.55 * k));
            p.o(J::Lift, Vec3::new(0.0, 0.18, -0.3) * k * rig.legs);
            let flap = (time * 4.0).sin();
            mantle(p, (1.0 + 0.35 * flap) * k);
            p.r(J::FL, fwd(1.0 * k));
            p.r(J::FR, fwd(1.0 * k));
            p.r(J::Neck, bow(-0.4 * k));
            p.r(J::Jaw, bow(0.5 * k));
        }
        (Style::Jabberwock, 0) => {
            // Jabbers: arms flailing, jaw going, bouncing on its haunches.
            let b = beat.sin();
            p.o(J::Lift, Vec3::Y * 0.06 * b.abs() * k);
            p.r(J::FL, fwd((1.2 + 0.6 * b) * k));
            p.r(J::FR, fwd((1.2 - 0.6 * b) * k));
            p.r(J::FLs, fwd(-0.8 * k));
            p.r(J::FRs, fwd(-0.8 * k));
            p.r(J::Neck, roll(0.3 * b * k) * bow(-0.3 * k));
            p.r(J::Jaw, bow((0.4 + 0.3 * (time * 18.0).sin()) * k));
        }
        (Style::Aquator, 0) => {
            // Frog hops on the spot.
            let hop = (time * 4.0).sin().max(0.0);
            p.o(J::Lift, Vec3::Y * 0.22 * hop * k);
            p.r(J::Body, bow(-0.25 * hop * k));
            for leg in [J::BL, J::BR] {
                p.r(leg, fwd(-0.7 * hop * k));
            }
        }
        (Style::Xeroc, 0) => {
            // Spins on its shell.
            p.r(J::Lift, turn(spins(progress, 3.0)));
            p.o(J::Lift, Vec3::Y * 0.05 * beat.sin().abs() * k);
            p.r(J::Jaw, bow(0.5 * k));
        }
        (Style::Xeroc, _) => {
            // Chomps at the air.
            p.r(J::Jaw, bow(0.9 * (time * 6.0).sin().abs() * k));
            p.r(J::Head, bow(-0.15 * k));
        }
        (Style::Dragon | Style::Griffin | Style::Jabberwock | Style::Aquator, _) => {
            zoomies(p, progress, 0.6, k);
            if rig.style == Style::Aquator {
                p.r(J::Lift, turn(spins(progress, 1.0)));
            }
        }
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------------------
// Segmented growths: a medusa's serpents and an aquator's tendrils.

pub(super) fn place(
    rig: &Rig,
    a: &Anim,
    fun: f32,
    joints: &mut Query<&mut Transform, Without<Rig>>,
) -> bool {
    let (m, t) = a
        .action
        .filter(|_| a.dead.is_none())
        .map_or((None, 0.0), |(m, t)| (Some(m), t));
    let d = ease(a.dead.unwrap_or(a.fallen));
    let alive = 1.0 - d;
    let (gather, strike) = m.map_or((0.0, 0.0), |m| {
        let (g, s, _) = timing(m, t);
        (g, if m.offensive() { s } else { 0.0 })
    });
    let time = a.time + a.seed * 2.3;
    let (w, gag, _) = antic(a);
    let fun = w * fun * f32::from(u8::from(m.is_none() && a.dead.is_none()));
    match rig.style {
        Style::Medusa => {
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                // Each serpent weaves on its own rhythm, rears at her gaze,
                // and slumps when she falls.
                let weave = Quat::from_rotation_x((time * 1.3 + f * 0.9).sin() * 0.22 * alive)
                    * Quat::from_rotation_z((time * 1.1 + f * 1.7).cos() * 0.25 * alive)
                    * Quat::from_rotation_y((time * 2.1 + f).sin() * 0.3 * fun);
                let rear = Quat::from_rotation_x(-0.35 * gather + 0.5 * strike);
                let slump = Quat::from_rotation_x(1.1 * d);
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.rotation = slump * rear * weave;
                }
            }
            true
        }
        Style::Aquator => {
            for (i, e) in rig.segments.iter().enumerate() {
                let (chain, k) = (i / 4, (i % 4) as f32);
                let side = if chain == 0 { -1.0 } else { 1.0 };
                let sway = (time * 1.4 - k * 0.7 + side).sin() * 0.25;
                // Tendrils sweep forward and up to make contact.
                let sweep = if k == 0.0 {
                    -0.6 * gather + 0.9 * strike
                } else {
                    0.15 * gather - 0.25 * strike
                };
                let twirl = if gag == 1 {
                    (time * 3.0 + k).sin() * 0.6 * fun
                } else {
                    0.0
                };
                let droop = 0.45 * d;
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.rotation = Quat::from_rotation_x(sweep + droop + 0.08 * k * alive)
                        * Quat::from_rotation_z(side * (sway * alive + twirl));
                }
            }
            true
        }
        _ => false,
    }
}
