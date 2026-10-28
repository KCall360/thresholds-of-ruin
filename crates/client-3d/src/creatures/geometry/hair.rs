use super::*;

/// The colour of this individual's hair, by kind.
pub(super) fn hair_color(l: &Look) -> Vec3 {
    let t = l.v.hair_tone as usize;
    match l.style {
        Style::Vampire => [HAIRS[0], HAIRS[0], HAIRS[1], HAIRS[5], HAIRS[0], HAIRS[1]][t],
        Style::Hobgoblin => [
            DARK * 3.0,
            HAIRS[0],
            HAIRS[1],
            DARK * 3.0,
            HAIRS[1],
            HAIRS[0],
        ][t],
        Style::Zombie => HAIRS[t] * 0.75 + Vec3::splat(0.05),
        Style::Orc | Style::Troll => {
            [HAIRS[0], HAIRS[1], DARK * 3.0, HAIRS[0], HAIRS[1], HAIRS[1]][t]
        }
        // Every leprechaun is a redhead, from copper to deep auburn.
        Style::Leprechaun => [
            Vec3::new(0.78, 0.32, 0.1),
            Vec3::new(0.7, 0.26, 0.08),
            Vec3::new(0.86, 0.4, 0.14),
            Vec3::new(0.6, 0.22, 0.08),
            Vec3::new(0.82, 0.36, 0.12),
            Vec3::new(0.66, 0.28, 0.12),
        ][t],
        Style::Nymph => [HAIRS[4], HAIRS[2], HAIRS[1], HAIRS[4], HAIRS[3], HAIRS[2]][t],
        // The dead go white: silver, ash and bone.
        Style::Phantom | Style::Wraith => [
            Vec3::new(0.86, 0.86, 0.9),
            Vec3::new(0.78, 0.8, 0.86),
            Vec3::new(0.9, 0.88, 0.84),
            Vec3::new(0.72, 0.74, 0.8),
            Vec3::new(0.84, 0.84, 0.86),
            Vec3::new(0.8, 0.78, 0.82),
        ][t],
        _ => HAIRS[t],
    }
}

/// A sculpted lock of hair: one solid clump, broad at the root and tapering
/// to its tip, laid along `path`.
pub(super) fn lock(b: &mut Builder, path: &[Vec3], width: f32, thick: f32, color: Vec3) {
    let n = path.len().max(2);
    let radii: Vec<Vec2> = (0..path.len())
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            Vec2::new(
                (width * (1.0 - 0.97 * t * t)).max(0.0015),
                (thick * (1.0 - 0.95 * t * t)).max(0.0015),
            )
        })
        .collect();
    // Broad side along the scalp: across the lock and around the head.
    let tangent = (path[1] - path[0]).normalize_or(Vec3::Y);
    let outward = (path[0] - Vec3::new(0.0, 0.17, -0.005)).normalize_or(Vec3::Y);
    let across = tangent.cross(outward).normalize_or(Vec3::X);
    b.tube_framed(path, &radii, 10, &|_, _, _| color, Some(across));
}

/// One strand of a sculpted hairstyle: points down its length and its
/// radius at the root and the tip.
pub(super) struct Strand {
    pub(super) attached: Vec<bool>,
    pub(super) support_side: Option<super::deformation::SupportSide>,
    pub(super) path: Vec<Vec3>,
    pub(super) root: f32,
    pub(super) tip: f32,
}

pub(super) const HAIR_SCALP_END_Y: f32 = 0.1;

pub(super) fn long_hair_strands(seed: u64) -> Vec<Strand> {
    let skull = Vec3::new(0.0, 0.17, -0.005);
    // The skull's horizontal radius at a height, a little proud of the scalp.
    let around = |y: f32| {
        let h = ((y - skull.y) / 0.118).clamp(-1.0, 1.0);
        (1.0 - h * h).max(0.0).sqrt()
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let mut strands = Vec::new();
    // Down the back, from one ear round to the other.
    let count = 11;
    for k in 0..count {
        let a = (k as f32 / (count - 1) as f32 - 0.5) * 2.3;
        let reach = -0.21 - 0.05 * rand(seed, 60 + k);
        let phase = rand(seed, 90 + k) * TAU;
        // Six segments carry its slow waves; each extra one costs every
        // sample of the sculpt.
        let path: Vec<Vec3> = (0..=6)
            .map(|i| {
                let s = i as f32 / 6.0;
                let y = lerp(0.27, reach, s);
                // Over the skull, then hanging clear of the neck and
                // spreading a little over the shoulder blades.
                let (wide, deep) = if y > HAIR_SCALP_END_Y {
                    let r = around(y);
                    (0.112 * r.max(0.62), 0.122 * r.max(0.62))
                } else {
                    let below = (HAIR_SCALP_END_Y - y) / (HAIR_SCALP_END_Y - reach);
                    (lerp(0.112 * 0.62, 0.15, below), lerp(0.138, 0.105, below))
                };
                // Slow, broad waves: the fall moves as one sheet of hair.
                let wave = (y * 9.0 + phase).sin() * 0.014 * s;
                Vec3::new(
                    a.sin() * wide + a.cos() * wave,
                    y,
                    skull.z - a.cos() * deep - 0.012 + a.sin().abs() * 0.02,
                )
            })
            .collect();
        strands.push(Strand {
            attached: path.iter().map(|p| p.y > HAIR_SCALP_END_Y).collect(),
            support_side: Some(super::deformation::SupportSide::Back),
            path,
            root: 0.03,
            tip: 0.008,
        });
    }
    for side in [-1.0f32, 1.0] {
        // Two locks spilling forward over each shoulder onto the chest, as
        // on the sheets: down past the ear, over the collarbone, and down.
        for (k, (depth, reach)) in [(0.03f32, -0.25f32), (-0.01, -0.21)]
            .into_iter()
            .enumerate()
        {
            let phase = rand(seed, 70 + k as u32 + side.max(0.0) as u32 * 4) * TAU;
            let path: Vec<Vec3> = (0..=6)
                .map(|i| {
                    let s = i as f32 / 6.0;
                    let y = lerp(0.22, reach, s);
                    let wave = (y * 10.0 + phase).sin() * 0.012 * s;
                    // Forward over the shoulder once past the jaw.
                    let over = ((s - 0.4) / 0.6).clamp(0.0, 1.0);
                    Vec3::new(
                        side * (0.097 + 0.035 * s.powf(0.7) - 0.02 * over + wave),
                        y,
                        depth - 0.03 * (1.0 - s) * (1.0 - s) + 0.1 * over * over,
                    )
                })
                .collect();
            strands.push(Strand {
                attached: path.iter().map(|p| p.y > HAIR_SCALP_END_Y).collect(),
                support_side: Some(super::deformation::SupportSide::Front),
                path,
                root: 0.03,
                tip: 0.013,
            });
        }
    }
    // A soft lock swept from the parting across the brow to the temple.
    let part = if seed.is_multiple_of(2) { 1.0f32 } else { -1.0 };
    strands.push(Strand {
        attached: vec![true; 5],
        support_side: None,
        path: vec![
            Vec3::new(part * 0.03, 0.3, 0.035),
            Vec3::new(-part * 0.02, 0.285, 0.085),
            Vec3::new(-part * 0.075, 0.25, 0.09),
            Vec3::new(-part * 0.103, 0.19, 0.06),
            Vec3::new(-part * 0.11, 0.12, 0.035),
        ],
        root: 0.026,
        tip: 0.014,
    });
    strands
}

/// Combed hair: fine streaks running down each strand, a soft sheen where
/// it turns to the light and darker undersides.
pub(super) fn hair_sheen(at: Vec3, normal: Vec3, seed: u64) -> f32 {
    let a = at.x.atan2(-at.z);
    let streak = 1.0
        + 0.07 * (a * 41.0 + at.y * 5.0 + seed as f32).sin()
        + 0.035 * (a * 97.0 - at.y * 3.0).sin();
    let sheen = 1.0 + 0.16 * normal.y.max(0.0).powi(2) * (1.0 - normal.z.abs());
    let under = if normal.y < -0.4 { 0.86 } else { 1.0 };
    0.94 * streak * sheen * under
}

/// A plait hanging from `top`, ending in a little tuft.
pub(super) fn braid(p: &mut Parts, top: Vec3, color: Vec3) {
    for k in 0..8 {
        let t = k as f32 / 7.0;
        let at = top
            + Vec3::new(
                if k % 2 == 0 { 0.008 } else { -0.008 },
                -t * 0.26,
                -t * 0.03,
            );
        p.matte.blob(
            at,
            Vec3::splat(0.032 - 0.012 * t),
            1,
            0.0,
            color * (if k % 2 == 0 { 1.0 } else { 0.92 }),
        );
    }
    lock(
        &mut p.matte,
        &[
            top + Vec3::new(0.0, -0.27, -0.03),
            top + Vec3::new(0.0, -0.32, -0.035),
        ],
        0.02,
        0.014,
        color,
    );
}

/// A short beard along the jaw to the chin, and a moustache.
pub(super) fn beard(p: &mut Parts, color: Vec3) {
    let c = color * 0.95;
    for side in [-1.0f32, 1.0] {
        lock(
            &mut p.matte,
            &[
                Vec3::new(side * 0.088, 0.13, 0.0),
                Vec3::new(side * 0.075, 0.08, 0.055),
                Vec3::new(side * 0.03, 0.055, 0.095),
                Vec3::new(0.0, 0.05, 0.1),
            ],
            0.026,
            0.02,
            c,
        );
        lock(
            &mut p.matte,
            &[
                Vec3::new(side * 0.004, 0.1, 0.112),
                Vec3::new(side * 0.025, 0.097, 0.108),
                Vec3::new(side * 0.034, 0.086, 0.1),
            ],
            0.01,
            0.008,
            c,
        );
    }
    p.matte.blob(
        Vec3::new(0.0, 0.058, 0.088),
        Vec3::new(0.036, 0.022, 0.026),
        2,
        0.0,
        c,
    );
}

/// A crown of sculpted clumps: broad locks radiating from the top of the
/// head down to the hairline, so the edge of the hair is a run of grouped
/// locks rather than the rim of a cap. `front`, `side` and `back` are the
/// heights the locks end at; `sweep` pushes them toward a parting.
#[allow(clippy::too_many_arguments)]
pub(super) fn crown(
    p: &mut Parts,
    tone: &dyn Fn(u32) -> Vec3,
    front: f32,
    side: f32,
    back: f32,
    sweep: f32,
    width: f32,
    count: u32,
) {
    let skull = Vec3::new(0.0, 0.17, -0.005);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU;
        let facing = a.cos(); // 1 at the front, -1 at the back.
        let end_y = if facing >= 0.0 {
            side + (front - side) * facing
        } else {
            side + (back - side) * -facing
        };
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let root = Vec3::new(0.0, 0.292, -0.015) + dir * 0.025;
        // Points on the skull surface, a little proud of it.
        let on_skull = |y: f32, out: f32| {
            let h = ((y - skull.y) / 0.112).clamp(-1.0, 1.0);
            let r = (1.0 - h * h).max(0.0).sqrt();
            skull + Vec3::new(dir.x * 0.1 * r, y - skull.y, dir.z * 0.108 * r) * out
        };
        let mid_y = (0.29 + end_y) * 0.5 + 0.02;
        let mid = on_skull(mid_y, 1.16) + Vec3::X * sweep * 0.5;
        let end = on_skull(end_y, 1.1) + Vec3::X * sweep;
        lock(&mut p.matte, &[root, mid, end], width, 0.03, tone(k));
    }
}

/// Hair as sculpted clumps over a fitted cap, with volume where the style
/// carries it. Men: swept, cropped, tousled, slicked, tied back. Women: long,
/// ponytail, bun, braid, bob, all falling over the ears and temples.
/// Which cut a person's hair takes: for men swept, cropped, tousled,
/// slicked or tied back; for women long, ponytail, bun, braid or bob.
pub(super) fn hair_style(l: &Look) -> u8 {
    let v = l.v;
    match l.style {
        Style::Vampire if !v.woman => 3,
        Style::Vampire => [0, 2, 0, 2, 4][v.hair as usize],
        Style::Zombie if !v.woman => 2,
        Style::Zombie => 0,
        // As drawn on their sheets: tousled or swept fey heads, a long
        // braid on her; the dead wear theirs long and loose.
        Style::Leprechaun if !v.woman => [2, 0, 2, 2, 0][v.hair as usize],
        Style::Leprechaun => 3,
        Style::Nymph if !v.woman => 2,
        Style::Nymph => 0,
        Style::Phantom if !v.woman => [4, 3, 4, 4, 3][v.hair as usize],
        Style::Phantom => 0,
        _ => v.hair,
    }
}

/// Whether a person's head is covered by a hood or helm rather than
/// showing hair.
pub(super) fn hooded(l: &Look) -> bool {
    matches!(
        Outfit::worn(l).head,
        Headwear::Hood { .. } | Headwear::Helm | Headwear::WingedHelm { .. }
    )
}

/// Whether a person shows hair at all: not under a hood, and not a kind
/// whose head is bald, crested or crowned with serpents.
pub(super) fn shows_hair(l: &Look) -> bool {
    !hooded(l) && grows_scalp_hair(l)
}

/// Species hair identity stays separate from whether headwear covers it.
pub(super) fn grows_scalp_hair(l: &Look) -> bool {
    !matches!(
        l.style,
        Style::Guardian
            | Style::Yeti
            | Style::Hobgoblin
            | Style::Orc
            | Style::Troll
            | Style::Medusa
    )
}

/// Whether a person wears long loose hair, which hangs from the crown on
/// its own and swings as she moves.
pub(in crate::creatures) fn loose_fall(l: &Look) -> bool {
    l.plan == Plan::Biped && l.v.woman && shows_hair(l) && hair_style(l) == 0
}

/// The point on the crown long hair hangs from, in the head's space.
pub(super) const CROWN: Vec3 = Vec3::new(0.0, 0.25, -0.05);

/// The long fall of loose hair as it would sit on the head, for tests.
#[cfg(test)]
pub(in crate::creatures) fn long_hair_form_for(p: &mut Parts, l: &Look) {
    long_fall(p, l);
}

/// Each lock has its own topology; the head's fitted cap covers their roots.
pub(super) fn long_fall(p: &mut Parts, l: &Look) {
    for strand in long_hair_strands(hair_seed(l, 0)) {
        hanging_lock(p, l, &strand);
    }
}

pub(super) fn hanging_lock(p: &mut Parts, l: &Look, strand: &Strand) {
    let color = hair_color(l);
    let seed = hair_seed(l, 0);
    // Linear subdivision stays inside the authored control envelope. Spline
    // overshoot would require a larger collider than the recipe advertises.
    let mut path = Vec::new();
    let mut radii = Vec::new();
    let last = strand.path.len() - 1;
    for (segment, pair) in strand.path.windows(2).enumerate() {
        for step in 0..4 {
            let u = step as f32 / 4.0;
            let t0 = segment as f32 / last as f32;
            let t1 = (segment + 1) as f32 / last as f32;
            let radius = |t: f32| strand.root + (strand.tip - strand.root) * t * t;
            path.push(pair[0].lerp(pair[1], u));
            radii.push(Vec2::splat(radius(t0) + (radius(t1) - radius(t0)) * u));
        }
    }
    path.push(*strand.path.last().unwrap());
    radii.push(Vec2::splat(strand.tip));
    let smooth = p.matte.smooth;
    let sides_mul = p.matte.sides_mul;
    p.matte.smooth = false;
    p.matte.sides_mul = 1;
    p.matte.tube_framed(
        &path,
        &radii,
        20,
        &|at, normal, _| color * hair_sheen(at, normal, seed),
        None,
    );
    p.matte.smooth = smooth;
    p.matte.sides_mul = sides_mul;
}

pub(super) fn hair_seed(l: &Look, style: u8) -> u64 {
    u64::from(l.v.hair_tone) * 31 + u64::from(style) + u64::from(l.v.skin) * 7
}

/// A person's hair, without the long fall when that hangs on its own.
pub(super) fn hair(p: &mut Parts, l: &Look, with_fall: bool) {
    let c = hair_color(l);
    let v = l.v;
    let style = hair_style(l);
    let seed = hair_seed(l, style);
    let tone = move |k: u32| c * (0.9 + 0.18 * rand(seed, k));
    let mass = |p: &mut Parts, at: Vec3, r: Vec3, color: Vec3| p.matte.blob(at, r, 2, 0.0, color);
    // The fitted cap under the clumps fills any gaps between them.
    mass(
        p,
        Vec3::new(0.0, 0.205, -0.02),
        Vec3::new(0.105, 0.094, 0.11),
        c * 0.9,
    );
    if !v.woman {
        match style {
            // Swept up and over: a quiff above a crown of clumps.
            0 => {
                crown(p, &tone, 0.245, 0.19, 0.13, 0.0, 0.062, 11);
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(-0.06, 0.27, 0.05),
                        Vec3::new(-0.01, 0.315, 0.085),
                        Vec3::new(0.05, 0.3, 0.1),
                        Vec3::new(0.085, 0.255, 0.085),
                    ],
                    0.055,
                    0.04,
                    tone(40),
                );
            }
            // Cropped short and close.
            1 => crown(p, &tone, 0.25, 0.2, 0.15, 0.0, 0.05, 13),
            // Tousled: uneven clumps, tips lifting.
            2 => {
                crown(p, &tone, 0.235, 0.19, 0.12, 0.012, 0.06, 10);
                for k in 0..6 {
                    let a = k as f32 * 2.4;
                    let at = Vec3::new(a.cos() * 0.05, 0.29, a.sin() * 0.055);
                    let out = Vec3::new(a.cos(), 0.5, a.sin()).normalize();
                    lock(
                        &mut p.matte,
                        &[at, at + out * 0.05, at + out * 0.08 - Vec3::Y * 0.015],
                        0.045,
                        0.026,
                        tone(20 + k),
                    );
                }
            }
            // Slicked straight back.
            3 => {
                crown(p, &tone, 0.24, 0.19, 0.12, 0.0, 0.07, 9);
                mass(
                    p,
                    Vec3::new(0.0, 0.25, -0.05),
                    Vec3::new(0.104, 0.06, 0.1),
                    c,
                );
            }
            // Swept back and tied at the nape.
            _ => {
                crown(p, &tone, 0.245, 0.19, 0.14, 0.0, 0.065, 10);
                p.matte.torus(
                    Vec3::new(0.0, 0.165, -0.13),
                    Vec3::new(0.0, 0.3, -1.0).normalize(),
                    0.02,
                    0.007,
                    10,
                    4,
                    c * 0.6,
                );
                // A slim tail that lies along the nape instead of jutting out.
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(0.0, 0.165, -0.13),
                        Vec3::new(0.0, 0.11, -0.148),
                        Vec3::new(0.0, 0.04, -0.142),
                        Vec3::new(0.0, -0.04, -0.125),
                    ],
                    0.034,
                    0.026,
                    c,
                );
            }
        }
        return;
    }
    // Hers: a side-parted crown falling over the ears to the nape.
    crown(p, &tone, 0.235, 0.12, 0.09, 0.015, 0.068, 12);
    match style {
        // Long and loose over the shoulders, locks framing the face: one
        // sculpted fall of hair whose waving strands merge into a soft mass
        // at the scalp and part into separate locks towards their tips.
        0 => {
            if with_fall {
                long_fall(p, l);
            }
        }
        // A high ponytail.
        1 => {
            p.matte.torus(
                Vec3::new(0.0, 0.215, -0.125),
                Vec3::new(0.0, 0.5, -1.0).normalize(),
                0.024,
                0.009,
                10,
                4,
                c * 0.6,
            );
            lock(
                &mut p.matte,
                &[
                    Vec3::new(0.0, 0.215, -0.135),
                    Vec3::new(0.0, 0.17, -0.21),
                    Vec3::new(0.0, 0.04, -0.21),
                    Vec3::new(0.0, -0.07, -0.16),
                ],
                0.06,
                0.05,
                c,
            );
        }
        // A neat bun.
        2 => {
            mass(
                p,
                Vec3::new(0.0, 0.25, -0.115),
                Vec3::new(0.062, 0.056, 0.056),
                tone(6),
            );
            p.matte.torus(
                Vec3::new(0.0, 0.25, -0.115),
                Vec3::new(0.0, 0.3, -1.0).normalize(),
                0.055,
                0.008,
                12,
                4,
                c * 0.75,
            );
        }
        // A single long braid.
        3 => braid(p, Vec3::new(0.0, 0.13, -0.13), c),
        // A bob to the jaw.
        _ => {
            for side in [-1.0f32, 1.0] {
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(side * 0.09, 0.25, 0.02),
                        Vec3::new(side * 0.115, 0.15, 0.02),
                        Vec3::new(side * 0.105, 0.07, 0.0),
                    ],
                    0.06,
                    0.04,
                    tone(50 + side as u32),
                );
            }
            // Locks round the back of the head curve in under the jaw line,
            // a softer, layered cut instead of one solid helmet of hair.
            for k in 0..7 {
                let a = (k as f32 / 6.0 - 0.5) * 2.6 + PI;
                let out = Vec3::new(a.sin(), 0.0, a.cos());
                let root = Vec3::new(out.x * 0.07, 0.24, out.z * 0.06 - 0.02);
                lock(
                    &mut p.matte,
                    &[
                        root,
                        Vec3::new(out.x * 0.11, 0.15, out.z * 0.11 - 0.02),
                        Vec3::new(out.x * 0.115, 0.07, out.z * 0.115 - 0.02),
                        Vec3::new(out.x * 0.095, 0.03, out.z * 0.095 - 0.02),
                    ],
                    0.056,
                    0.034,
                    tone(80 + k),
                );
            }
            // A side-swept fringe over the brow.
            lock(
                &mut p.matte,
                &[
                    Vec3::new(-0.06, 0.272, 0.04),
                    Vec3::new(0.0, 0.258, 0.072),
                    Vec3::new(0.055, 0.228, 0.083),
                ],
                0.044,
                0.016,
                tone(90),
            );
        }
    }
}
