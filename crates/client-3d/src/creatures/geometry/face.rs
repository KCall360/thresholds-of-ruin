use super::*;

pub(super) fn eyes(p: &mut Parts, accent: Option<Accent>, center: Vec3, spread: f32, size: f32) {
    if let Some(accent) = accent {
        p.accent = Some(accent);
        for side in [-1.0, 1.0] {
            p.glow.blob(
                center + Vec3::X * side * spread,
                Vec3::splat(size),
                0,
                0.0,
                Vec3::ONE,
            );
        }
    } else {
        cartoon_eyes(p, center, spread, size, Vec3::new(0.36, 0.22, 0.11));
    }
}

/// A face of moderate, modelled features: skull and jaw, cheekbones, a nose
/// with a bridge and wings, lidded eyes, brows, lips and ears. Women's faces
/// are softer in the jaw, with finer brows, lashes and fuller lips.
/// A sculpted human head and the mouth it carries.
pub(in crate::creatures) struct FaceShape {
    pub form: crate::sdf::Sculpt,
    pub mouth_width: f32,
    /// How far the mouth corners lift, and how far one side does.
    pub smile: f32,
    pub smirk: f32,
}

pub(in crate::creatures) fn face_shape(l: &Look, nose: bool) -> FaceShape {
    let fem = l.v.woman;
    // The head is sculpted as one surface: cranium and jaw blended,
    // cheekbones and a brow over carved eye sockets, a nose that grows out of
    // the face with its wings, lips with a fitted mouth line, a chin, ears.
    use crate::sdf::{Sculpt, Shape::*};
    // Her nose is finer and narrower, as drawn on the sheets.
    let n = if fem { 0.76 } else { 1.0 };
    let gaunt = matches!(
        l.style,
        Style::Zombie | Style::Vampire | Style::Phantom | Style::Wraith
    );
    let brute = matches!(l.style, Style::Hobgoblin | Style::Orc | Style::Troll);
    // A living woman's face is soft and rounded, as on the concept sheets:
    // full lower cheeks, a small chin and full lips. The undead keep their
    // hollows and the brutes their heavy planes.
    let soft = fem && !gaunt && !brute;
    let mut jaw = if fem {
        Vec3::new(0.069, 0.066, 0.075)
    } else {
        [
            Vec3::new(0.078, 0.067, 0.078),
            Vec3::new(0.083, 0.069, 0.08),
            Vec3::new(0.089, 0.07, 0.082),
        ][l.v.build as usize]
    };
    if fem {
        jaw.x *= [0.96, 1.0, 1.06][l.v.build as usize];
    }
    if gaunt {
        jaw *= Vec3::new(0.87, 1.0, 0.95);
    } else if brute {
        jaw *= Vec3::new(1.12, 0.96, 1.04);
    }
    let mut head = Sculpt::default();
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.182, -0.012),
            Vec3::new(0.096, 0.102, 0.102),
        ),
        0.0,
    )
    .add(Ellipsoid(Vec3::new(0.0, 0.112, 0.025), jaw), 0.022)
    .add(
        Ellipsoid(
            // Taller support joins the lower lip to the jaw without changing
            // the chin's forward plane, width, or lower extent.
            Vec3::new(0.0, 0.065, if soft { 0.07 } else { 0.068 }),
            Vec3::new(0.037, 0.031, 0.027)
                * if soft {
                    Vec3::new(0.8, 0.88, 0.95)
                } else if fem {
                    Vec3::splat(0.9)
                } else {
                    Vec3::ONE
                },
        ),
        0.024,
    )
    .add(
        Capsule(
            Vec3::new(0.0, 0.02, -0.01),
            Vec3::new(0.0, 0.1, -0.01),
            0.044 * n,
            0.046,
        ),
        0.03,
    );
    for side in [-1.0f32, 1.0] {
        // A broad malar pad flows into the midcheek. A thin raised ridge
        // over an empty lower cheek ages healthy faces at portrait distance.
        head.add(
            Ellipsoid(
                Vec3::new(side * 0.053, if gaunt { 0.135 } else { 0.13 }, 0.055),
                if gaunt {
                    Vec3::new(0.03, 0.022, 0.025)
                } else {
                    Vec3::new(0.032, 0.027, 0.027)
                },
            ),
            if gaunt { 0.024 } else { 0.03 },
        )
        .add(
            Ellipsoid(
                Vec3::new(side * 0.098, 0.15, -0.005),
                Vec3::new(0.014, 0.03, 0.022),
            ),
            0.01,
        );
    }
    if soft {
        // Full, soft cheeks round the face under the cheekbones, instead of
        // a flat front meeting flat sides; held above the jaw line so they
        // never sag into jowls.
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.046, 0.108, 0.061),
                    Vec3::new(0.031, 0.027, 0.03),
                ),
                0.03,
            );
        }
    }
    // The brow, heavier on men.
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.186, 0.078),
            Vec3::new(0.072, if brute { 0.022 } else { 0.013 }, 0.022)
                * if fem && !brute { 0.9 } else { 1.0 },
        ),
        0.012,
    );
    if nose {
        head.add(
            Capsule(
                Vec3::new(0.0, 0.18, 0.094),
                Vec3::new(0.0, 0.135, 0.118),
                0.009 * n,
                0.014 * n,
            ),
            0.012,
        )
        .add(
            Ellipsoid(
                Vec3::new(0.0, 0.13, 0.117),
                Vec3::new(0.018, 0.015, 0.016) * n,
            ),
            0.008,
        );
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.014 * n, 0.126, 0.106),
                    Vec3::splat(0.011 * n),
                ),
                0.006,
            );
        }
    }
    // A shallow perioral plane supports the lips and flows into the chin.
    // Keep its sides inside the cheek planes, including the undead hollows.
    // On a soft face it is wider and shallower, melting into the cheeks so
    // no edge of it shows beside the mouth corners.
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.088, 0.071),
            if soft {
                Vec3::new(0.05, 0.028, 0.023)
            } else {
                Vec3::new(if gaunt { 0.035 } else { 0.04 }, 0.028, 0.025)
            },
        ),
        if soft { 0.028 } else { 0.014 },
    );
    // Lip heights exceed the surface-net spacing; narrow carved slits below
    // that spacing produce alternating normals and dark, broken mouth corners.
    let mouth_width = if brute {
        0.032
    } else if gaunt {
        0.027
    } else {
        0.03
    };
    let full = if soft {
        1.5
    } else if fem {
        1.25
    } else {
        1.0
    };
    let pout = if soft { 0.0055 } else { 0.0 };
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.098, 0.093 + pout * 0.5),
            Vec3::new(mouth_width, 0.0065 * full, 0.007 + pout),
        ),
        0.01,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.086, 0.091 + pout),
            Vec3::new(mouth_width * 0.92, 0.007 * full, 0.007 + pout),
        ),
        0.01,
    );
    if soft {
        // The cupid's bow: the upper lip's two peaks either side of the
        // philtrum.
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.008, 0.1025, 0.095),
                    Vec3::new(0.009, 0.004, 0.005),
                ),
                0.004,
            );
        }
    }
    // Each kind wears its own expression: adventurers half-smile, soldiers
    // and hobgoblins scowl, vampires smirk to one side, the dead hang slack.
    let (smile, _scowl, smirk) = match l.style {
        Style::Hero | Style::Scout | Style::Caster | Style::Common | Style::Centaur => {
            (0.006, -0.004, 0.0)
        }
        // Fey grin; the medusa smirks like the vampire.
        Style::Leprechaun | Style::Nymph => (0.009, -0.004, 0.0),
        Style::Hobgoblin | Style::Sentinel | Style::Orc | Style::Troll => (-0.004, 0.009, 0.0),
        Style::Vampire | Style::Medusa => (0.0, 0.005, 0.006),
        Style::Phantom | Style::Wraith => (-0.003, 0.004, 0.0),
        Style::Zombie => (-0.006, -0.006, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    // The expression follows the continuous lip surface. Its narrow seam is
    // explicit fitted geometry below, independent of the face mesh grid.
    if smile > 0.0 {
        // Cheeks lift with the smile.
        for side in [-1.0f32, 1.0] {
            // On a soft face the lift melts into the full cheek around it.
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.036, 0.104, 0.082),
                    if soft {
                        Vec3::new(0.009, 0.007, 0.007)
                    } else {
                        Vec3::new(0.012, 0.009, 0.009)
                    },
                ),
                if soft { 0.02 } else { 0.014 },
            );
        }
    }
    for side in [-1.0f32, 1.0] {
        // Broad cheek planes and a defined chin keep the larger storybook
        // head adult. Undead have hollow lower cheeks, not a healthy round
        // face recoloured green; their skull still remains one closed form.
        head.carve(
            Beyond(
                Vec3::new(side * 0.8, 0.0, 0.6),
                if brute {
                    0.113
                } else if soft {
                    0.116
                } else {
                    0.106
                },
            ),
            if soft { 0.028 } else { 0.01 },
        );
        if gaunt {
            // A shallow, tall hollow below the cheekbone reads as a lean
            // facial plane rather than a second round eye socket.
            let hollow = if l.style == Style::Zombie {
                0.026
            } else {
                0.021
            };
            head.carve(
                Ellipsoid(
                    Vec3::new(side * 0.085, 0.105, 0.074),
                    Vec3::new(hollow, 0.037, 0.019),
                ),
                0.015,
            );
        }
        // Eye sockets under the brow.
        head.carve(
            Ellipsoid(
                Vec3::new(side * 0.038, 0.16, 0.1),
                Vec3::new(0.023, 0.017, 0.016),
            ),
            0.01,
        );
    }
    FaceShape {
        form: head,
        mouth_width,
        smile,
        smirk,
    }
}

pub(super) fn human_face(p: &mut Parts, l: &Look, nose: bool) {
    let fem = l.v.woman;
    let face = l.skin;
    let gaunt = matches!(
        l.style,
        Style::Zombie | Style::Vampire | Style::Phantom | Style::Wraith
    );
    let FaceShape {
        form: head,
        mouth_width,
        smile,
        smirk,
    } = face_shape(l, nose);
    let mouth_height = |x: f32| {
        let span = (x.abs() / mouth_width).min(1.0);
        0.091 + (0.001 + smile) * span.powi(2) + smirk * (x / mouth_width).clamp(0.0, 1.0)
    };
    // Women's lips are full and rose-coloured, their cheeks warmly flushed;
    // the undead keep a cold, dark mouth either way.
    let lips = if gaunt {
        (face * 0.6).lerp(Vec3::new(0.42, 0.18, 0.3), if fem { 0.3 } else { 0.12 })
    } else if fem {
        (face * 0.78).lerp(Vec3::new(0.78, 0.3, 0.36), 0.5)
    } else {
        (face * 0.8).lerp(Vec3::new(0.72, 0.36, 0.36), 0.15)
    };
    let flush = if gaunt {
        face * 0.96
    } else {
        face.lerp(Vec3::new(0.88, 0.45, 0.44), if fem { 0.2 } else { 0.05 })
    };
    head.mesh(
        &mut p.matte,
        Vec3::new(-0.125, 0.0, -0.13),
        Vec3::new(0.125, 0.3, 0.14),
        0.0055,
        &|at, _| {
            let mouth_y = mouth_height(at.x);
            let front = ((at.z - 0.075) / 0.015).clamp(0.0, 1.0);
            let lip =
                (1.2 - (at.x / (mouth_width + 0.002)).powi(2) - ((at.y - mouth_y) / 0.016).powi(2))
                    .clamp(0.0, 1.0)
                    * front;
            let cheek = (1.0 - Vec2::new(at.x.abs() - 0.055, at.y - 0.122).length() / 0.03)
                .clamp(0.0, 1.0)
                * f32::from(u8::from(at.z > 0.05));
            let mottle = 1.0
                + 0.008
                    * ((at.x * 173.0).sin() * (at.y * 151.0).cos()
                        + (at.z * 197.0 + at.x * 61.0).sin() * 0.5);
            let warm = ((1.0 - Vec2::new(at.x, at.y - 0.13).length() / 0.02).clamp(0.0, 1.0)
                * f32::from(u8::from(at.z > 0.1)))
            .max(
                (1.0 - (at.x.abs() - 0.098).abs() / 0.015).clamp(0.0, 1.0)
                    * f32::from(u8::from((at.y - 0.15).abs() < 0.03))
                    * 0.6,
            );
            let base = face.lerp(flush, cheek.max(warm * 0.6)).lerp(lips, lip);
            base * mottle
        },
    );
    let slack = l.style == Style::Zombie;
    let mouth_path: [Vec2; 25] = std::array::from_fn(|i| {
        let u = i as f32 / 24.0 * 2.0 - 1.0;
        if slack {
            Vec2::new(0.004 + u * 0.018, 0.089)
        } else {
            let x = u * mouth_width;
            Vec2::new(x, mouth_height(x))
        }
    });
    surface_stroke(
        &mut p.matte,
        &mouth_path,
        if slack {
            0.009
        } else if fem {
            0.00155
        } else {
            0.0014
        },
        &|at| torso_surface_frame(&head, at, true),
        if slack { DARK * 2.0 } else { lips * 0.6 },
    );
    if l.style == Style::Vampire {
        for side in [-1.0f32, 1.0] {
            let x = side * 0.018;
            let (root, normal) = torso_surface_frame(&head, Vec2::new(x, mouth_height(x)), true);
            p.matte.prism(
                root - normal * 0.002,
                Vec3::new(x, 0.06, 0.1),
                0.006,
                0.0,
                3,
                BONE,
                0.0,
            );
        }
    }
    let iris = [
        Vec3::new(0.36, 0.22, 0.11),
        Vec3::new(0.2, 0.12, 0.07),
        Vec3::new(0.45, 0.4, 0.18),
        Vec3::new(0.25, 0.45, 0.25),
        Vec3::new(0.25, 0.4, 0.62),
    ][((l.v.pattern + l.v.skin) % 5) as usize];
    // Her eyes are a little larger, as on the concept sheets. Eyes that glow
    // keep their lids and lashes, the light held in the iris.
    soft_eyes(
        p,
        Vec3::new(0.0, 0.16, 0.088),
        0.038,
        if fem { 0.0158 } else { 0.0145 },
        iris,
        l,
        &head,
    );
    face_brows(p, l, &head);
}

pub(super) fn face_brows(p: &mut Parts, l: &Look, head: &crate::sdf::Sculpt) {
    let brow = hair_color(l) * 0.85;
    for side in [-1.0f32, 1.0] {
        let profile = brow_profile(l, side);
        if l.v.woman {
            fitted_brow(&mut p.matte, head, &profile, brow);
            continue;
        }
        let points: Vec<Vec3> = profile.iter().map(|(at, _)| *at).collect();
        let radii: Vec<(f32, f32)> = profile.iter().map(|(_, r)| *r).collect();
        tube(&mut p.matte, &points, &radii, brow);
    }
}

/// A shallow tapered brow band. Fit every edge to the forehead, rather than
/// floating a wider tube above it or burying its sides in a curved temple.
pub(super) fn fitted_brow(
    b: &mut Builder,
    head: &crate::sdf::Sculpt,
    profile: &[(Vec3, (f32, f32))],
    color: Vec3,
) {
    let mut path = Vec::new();
    for pair in profile.windows(2) {
        for row in 0..4 {
            let t = row as f32 / 4.0;
            path.push((
                pair[0].0.truncate().lerp(pair[1].0.truncate(), t),
                Vec2::from(pair[0].1).lerp(Vec2::from(pair[1].1), t),
            ));
        }
    }
    let Some(&(end, radii)) = profile.last() else {
        return;
    };
    path.push((end.truncate(), Vec2::from(radii)));
    let mut grid = Vec::with_capacity(path.len() * 5);
    for (i, &(at, radii)) in path.iter().enumerate() {
        let tangent = (path[(i + 1).min(path.len() - 1)].0 - path[i.saturating_sub(1)].0)
            .normalize_or(Vec2::X);
        let across = Vec2::new(-tangent.y, tangent.x);
        for column in 0..=4 {
            let v = column as f32 * 0.5 - 1.0;
            let (surface, normal) = torso_surface_frame(head, at + across * radii.x * v, true);
            grid.push([
                surface + normal * 0.0014,
                surface + normal * (0.002 + radii.y * (1.0 - v * v)),
            ]);
        }
    }
    for row in 0..path.len() - 1 {
        for column in 0..4 {
            let q = [
                grid[row * 5 + column],
                grid[(row + 1) * 5 + column],
                grid[(row + 1) * 5 + column + 1],
                grid[row * 5 + column + 1],
            ];
            let center = q.iter().flatten().copied().sum::<Vec3>() * 0.125;
            let outward = q
                .iter()
                .map(|pair| pair[1] - pair[0])
                .sum::<Vec3>()
                .normalize_or(Vec3::Z);
            b.quad_out(q.map(|pair| pair[1]), center - outward * 0.01, color);
            b.quad_out(q.map(|pair| pair[0]), center + outward * 0.01, color);
            if column == 0 {
                b.quad_out([q[0][0], q[1][0], q[1][1], q[0][1]], center, color);
            }
            if column == 3 {
                b.quad_out([q[3][0], q[2][0], q[2][1], q[3][1]], center, color);
            }
            if row == 0 {
                b.quad_out([q[0][0], q[3][0], q[3][1], q[0][1]], center, color);
            }
            if row + 2 == path.len() {
                b.quad_out([q[1][0], q[2][0], q[2][1], q[1][1]], center, color);
            }
        }
    }
}

/// One eyebrow on `side`: points along it from the inner end to the outer
/// and its half-width and half-thickness at each. Adult female brows have a
/// substantial inner third, a controlled arch and a fine outer taper. Species
/// expressions lower the inner end; actual forehead fitting supplies depth.
pub(in crate::creatures) fn brow_profile(l: &Look, side: f32) -> Vec<(Vec3, (f32, f32))> {
    let fem = l.v.woman;
    let scowl = match l.style {
        Style::Hobgoblin | Style::Sentinel | Style::Orc | Style::Troll => 0.009,
        Style::Vampire | Style::Medusa | Style::Wraith => 0.005,
        Style::Zombie => -0.004,
        _ => -0.002,
    } * if fem { 0.6 } else { 1.0 };
    if fem {
        let (w, h) = (0.0048, 0.0025);
        vec![
            (
                Vec3::new(side * 0.017, 0.192 - scowl, 0.101),
                (w * 0.65, h * 0.65),
            ),
            (Vec3::new(side * 0.033, 0.196 - scowl * 0.3, 0.1), (w, h)),
            (Vec3::new(side * 0.051, 0.197, 0.097), (w * 0.85, h * 0.85)),
            (
                Vec3::new(side * 0.068, 0.191 + scowl * 0.2, 0.089),
                (w * 0.25, h * 0.25),
            ),
        ]
    } else {
        let (w, h) = (0.009, 0.006);
        vec![
            (Vec3::new(side * 0.018, 0.187 - scowl, 0.101), (w, h)),
            (Vec3::new(side * 0.042, 0.197 - scowl * 0.3, 0.099), (w, h)),
            (
                Vec3::new(side * 0.066, 0.189 + scowl * 0.3, 0.09),
                (w * 0.6, h * 0.6),
            ),
        ]
    }
}

/// A small, closed surface accent with a tapered cross-section. Project each
/// edge independently so a mouth stays seated on the actual lip curvature.
/// Explicit geometry resolves the seam without a sub-grid carved cavity.
pub(super) fn surface_stroke(
    b: &mut Builder,
    path: &[Vec2],
    half_width: f32,
    surface: &dyn Fn(Vec2) -> (Vec3, Vec3),
    color: Vec3,
) {
    if path.len() < 2 {
        return;
    }
    // Wide accents, such as a slack open mouth, need samples across the lip
    // bulge as well as along the expression curve. Narrow seams stay one cell.
    let across_steps = ((2.0 * half_width / 0.004).ceil() as usize).clamp(1, 5);
    let stride = across_steps + 1;
    let mut grid = Vec::with_capacity(path.len() * stride);
    for (i, &at) in path.iter().enumerate() {
        let tangent =
            (path[(i + 1).min(path.len() - 1)] - path[i.saturating_sub(1)]).normalize_or(Vec2::X);
        let across = Vec2::new(-tangent.y, tangent.x);
        let u = i as f32 / (path.len() - 1) as f32 * 2.0 - 1.0;
        let width = half_width * (1.0 - u * u).max(0.0).sqrt().max(0.08);
        for j in 0..=across_steps {
            let v = j as f32 / across_steps as f32 * 2.0 - 1.0;
            let (point, normal) = surface(at + across * (width * v));
            // Face surface nets average crossings inside a convex lip. The
            // normal clearance also covers their small local chord error.
            grid.push([point + normal * 0.0018, point + normal * (0.0018 + 0.0007)]);
        }
    }
    for i in 0..path.len() - 1 {
        for j in 0..across_steps {
            let q = [
                grid[i * stride + j],
                grid[(i + 1) * stride + j],
                grid[(i + 1) * stride + j + 1],
                grid[i * stride + j + 1],
            ];
            let center = q.iter().flatten().copied().sum::<Vec3>() * 0.125;
            let outward = q
                .iter()
                .map(|pair| pair[1] - pair[0])
                .sum::<Vec3>()
                .normalize_or(Vec3::Z);
            // Use the actual surface normal for front/back winding even if
            // the four fitted corners are not perfectly coplanar.
            b.quad_out(q.map(|pair| pair[1]), center - outward * 0.01, color);
            b.quad_out(q.map(|pair| pair[0]), center + outward * 0.01, color);
            if j == 0 {
                b.quad_out([q[0][0], q[1][0], q[1][1], q[0][1]], center, color);
            }
            if j + 1 == across_steps {
                b.quad_out([q[3][0], q[2][0], q[2][1], q[3][1]], center, color);
            }
            if i == 0 {
                b.quad_out([q[0][0], q[3][0], q[3][1], q[0][1]], center, color);
            }
            if i + 2 == path.len() {
                b.quad_out([q[1][0], q[2][0], q[2][1], q[1][1]], center, color);
            }
        }
    }
}

/// The authored globe follows the local socket plane. All visible pigments
/// share its surface and the same piecewise-linear almond boundary.
pub(super) struct EyeSurface<'a> {
    pub(super) head: &'a crate::sdf::Sculpt,
    pub(super) at: Vec3,
    pub(super) slope: Vec2,
    pub(super) size: f32,
    pub(super) side: f32,
}

impl<'a> EyeSurface<'a> {
    pub(super) const COLUMNS: usize = 32;

    pub(super) fn new(head: &'a crate::sdf::Sculpt, at: Vec3, size: f32, side: f32) -> Self {
        let center = torso_surface(head, at.x, at.y, true);
        let e = size * 0.7;
        let depth = |x, y| torso_surface(head, x, y, true).z;
        let slope = Vec2::new(
            (depth(at.x + e, at.y) - depth(at.x - e, at.y)) / (2.0 * e),
            (depth(at.x, at.y + e) - depth(at.x, at.y - e)) / (2.0 * e),
        )
        .clamp(Vec2::splat(-0.7), Vec2::splat(0.7));
        Self {
            head,
            at: center + Vec3::Z * 0.0012,
            slope,
            size,
            side,
        }
    }

    pub(super) fn half_width(&self) -> f32 {
        self.size * 1.35
    }

    pub(super) fn opening(&self, x: f32) -> (f32, f32) {
        // Interpolate the actual white-mesh boundary, rather than clipping a
        // disc against a slightly different analytic curve between columns.
        let column = ((x - self.at.x) / self.half_width() + 1.0) * 0.5 * Self::COLUMNS as f32;
        let left = column.floor().clamp(0.0, (Self::COLUMNS - 1) as f32);
        let t = (column - left).clamp(0.0, 1.0);
        let bounds = |column: f32| {
            let u = column / Self::COLUMNS as f32 * 2.0 - 1.0;
            let tilt = self.side * u * 0.1;
            Vec2::new(-0.39 * (1.0 - u * u) + tilt, 0.58 * (1.0 - u * u) + tilt) * self.size
                + Vec2::splat(self.at.y)
        };
        bounds(left).lerp(bounds(left + 1.0), t).into()
    }

    pub(super) fn point(&self, xy: Vec2, offset: f32) -> Vec3 {
        let delta = xy - self.at.truncate();
        let u = delta.x / self.half_width();
        let v = (delta.y - self.side * u * self.size * 0.1) / (self.size * 0.9);
        let globe = self.at.z
            + self.slope.dot(delta)
            + self.size * 0.5 * (1.0 - u * u - v * v).max(0.0).sqrt();
        let skin = torso_surface(self.head, xy.x, xy.y, true).z;
        Vec3::new(xy.x, xy.y, globe.max(skin + 0.0008) + offset)
    }
}

/// Close a sampled surface with a shallow back and side walls. The grid runs
/// along rows, with `columns + 1` points across each row.
pub(super) fn closed_eye_grid(b: &mut Builder, grid: &[[Vec3; 2]], columns: usize, color: Vec3) {
    let stride = columns + 1;
    let rows = grid.len() / stride;
    for row in 0..rows - 1 {
        for column in 0..columns {
            let q = [
                grid[row * stride + column],
                grid[(row + 1) * stride + column],
                grid[(row + 1) * stride + column + 1],
                grid[row * stride + column + 1],
            ];
            let center = q.iter().flatten().copied().sum::<Vec3>() * 0.125;
            b.quad_out(q.map(|p| p[1]), center - Vec3::Z * 0.01, color);
            b.quad_out(q.map(|p| p[0]), center + Vec3::Z * 0.01, color);
            if column == 0 {
                b.quad_out([q[0][0], q[1][0], q[1][1], q[0][1]], center, color);
            }
            if column + 1 == columns {
                b.quad_out([q[3][0], q[2][0], q[2][1], q[3][1]], center, color);
            }
            if row == 0 {
                b.quad_out([q[0][0], q[3][0], q[3][1], q[0][1]], center, color);
            }
            if row + 2 == rows {
                b.quad_out([q[1][0], q[2][0], q[2][1], q[1][1]], center, color);
            }
        }
    }
}

/// Paint a white or elliptical disc on the globe, clipping every layer to the
/// emitted opening. Shared column breaks keep the tessellated edges aligned.
pub(super) fn eye_patch(
    b: &mut Builder,
    eye: &EyeSurface,
    disc: Option<(Vec2, f32)>,
    offset: f32,
    color: Vec3,
) {
    let radius = disc.map_or(eye.half_width(), |(r, _)| r.x);
    let mut xs = vec![-radius, radius];
    for column in 0..=EyeSurface::COLUMNS {
        let x = (column as f32 / EyeSurface::COLUMNS as f32 * 2.0 - 1.0) * eye.half_width();
        if x > -radius && x < radius {
            xs.push(x);
        }
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let mut grid = Vec::with_capacity(xs.len() * 9);
    for x in xs {
        let (mut low, mut high) = eye.opening(eye.at.x + x);
        if let Some((r, y)) = disc {
            let extent = r.y * (1.0 - (x / r.x).powi(2)).max(0.0).sqrt();
            low = low.max(eye.at.y + y - extent);
            high = high.min(eye.at.y + y + extent);
        }
        for row in 0..=8 {
            let y = low + (high - low) * row as f32 / 8.0;
            let point = eye.point(Vec2::new(eye.at.x + x, y), offset);
            grid.push([point - Vec3::Z * 0.00025, point]);
        }
    }
    closed_eye_grid(b, &grid, 8, color);
}

pub(super) fn eye_lid(b: &mut Builder, eye: &EyeSurface, upper: bool, color: Vec3) {
    let mut grid = Vec::with_capacity((EyeSurface::COLUMNS + 1) * 5);
    for row in 0..=EyeSurface::COLUMNS {
        let u = row as f32 / EyeSurface::COLUMNS as f32 * 2.0 - 1.0;
        let x = eye.at.x + eye.half_width() * u;
        let (low, high) = eye.opening(x);
        let inner = eye.point(Vec2::new(x, if upper { high } else { low }), 0.0002);
        let outer_xy = eye.at.truncate()
            + Vec2::new(
                eye.size * 1.6 * u,
                eye.size
                    * (eye.side * u * 0.1
                        + if upper { 0.98 } else { -0.9 } * (1.0 - u * u).max(0.0).sqrt()),
            );
        let outer = torso_surface(eye.head, outer_xy.x, outer_xy.y, true) + Vec3::Z * 0.0012;
        for column in 0..=4 {
            let v = column as f32 / 4.0;
            let point = inner.lerp(outer, v) + Vec3::Z * (0.001 * (v * PI).sin());
            grid.push([point - Vec3::Z * 0.0007, point]);
        }
    }
    closed_eye_grid(b, &grid, 4, color);
}

pub(super) fn adult_eye(
    p: &mut Parts,
    eye: &EyeSurface,
    iris: Vec3,
    pupil: Vec2,
    face: Vec3,
    glow: Option<Accent>,
) {
    eye_patch(
        &mut p.matte,
        eye,
        None,
        0.0,
        if glow.is_some() {
            face * 0.3
        } else {
            Vec3::new(0.82, 0.81, 0.76)
        },
    );
    eye_patch(
        if glow.is_some() {
            &mut p.glow
        } else {
            &mut p.matte
        },
        eye,
        Some((Vec2::splat(eye.size * 0.7), -eye.size * 0.04)),
        0.0005,
        if glow.is_some() { Vec3::ONE } else { iris },
    );
    eye_patch(
        &mut p.matte,
        eye,
        Some((pupil, -eye.size * 0.04)),
        0.001,
        DARK,
    );
    eye_lid(
        &mut p.matte,
        eye,
        true,
        face.lerp(Vec3::new(0.55, 0.36, 0.42), 0.12),
    );
    eye_lid(&mut p.matte, eye, false, face * 0.98);
    let mut line = Vec::new();
    let mut radii = Vec::new();
    for k in 0..=EyeSurface::COLUMNS {
        let u = k as f32 / EyeSurface::COLUMNS as f32 * 2.0 - 1.0;
        let x = eye.at.x + eye.half_width() * u;
        line.push(eye.point(Vec2::new(x, eye.opening(x).1), 0.0016));
        let radius = eye.size * (0.04 + 0.085 * (1.0 - u * u));
        radii.push((radius, radius));
    }
    tube(&mut p.matte, &line, &radii, DARK);
    for u in [0.65, 0.88, 1.0] {
        let x = eye.at.x + eye.side * eye.half_width() * u;
        let root = eye.point(Vec2::new(x, eye.opening(x).1), 0.0016);
        p.matte.prism(
            root,
            root + Vec3::new(eye.side * eye.size * 0.22, eye.size * 0.18, 0.0005),
            eye.size * 0.035,
            0.0,
            3,
            DARK,
            0.0,
        );
    }
    let glint_xy = eye.at.truncate() + Vec2::new(-0.18, 0.1) * eye.size;
    p.matte.blob(
        eye.point(glint_xy, 0.0015),
        Vec3::splat(eye.size * 0.075),
        0,
        0.0,
        Vec3::splat(1.3),
    );
}

/// Adult female eyes use one fitted opening and lid assembly. Male eyes retain
/// their established construction; glow ownership stays restricted to the iris.
pub(super) fn soft_eyes(
    p: &mut Parts,
    center: Vec3,
    spread: f32,
    size: f32,
    iris: Vec3,
    l: &Look,
    head: &crate::sdf::Sculpt,
) {
    let (face, lashes, glow) = (l.skin, l.v.woman, l.eyes);
    if glow.is_some() {
        p.accent = glow;
    }
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        if lashes {
            let pupil = if l.style == Style::Medusa {
                Vec2::new(0.26, 0.43)
            } else {
                Vec2::splat(0.3)
            } * size;
            adult_eye(
                p,
                &EyeSurface::new(head, at, size, side),
                iris,
                pupil,
                face,
                glow,
            );
            continue;
        }
        p.matte.blob(
            at,
            Vec3::new(size * 1.35, size * 0.68, size * 0.65),
            0,
            0.0,
            if glow.is_some() {
                face * 0.3
            } else {
                Vec3::new(0.82, 0.81, 0.76)
            },
        );
        let iris_builder = if glow.is_some() {
            &mut p.glow
        } else {
            &mut p.matte
        };
        iris_builder.blob(
            at + Vec3::new(0.0, -size * 0.05, size * 0.5),
            Vec3::new(size * 0.7, size * 0.65, size * 0.3),
            0,
            0.0,
            if glow.is_some() { Vec3::ONE } else { iris },
        );
        p.matte.blob(
            at + Vec3::new(0.0, -size * 0.05, size * 0.68),
            Vec3::new(size * 0.32, size * 0.43, size * 0.18),
            0,
            0.0,
            DARK,
        );
        p.matte.blob(
            at + Vec3::new(size * 0.3, size * 0.3, size * 0.85),
            Vec3::splat(size * 0.12),
            0,
            0.0,
            Vec3::splat(1.6),
        );
        p.matte.blob(
            at + Vec3::new(0.0, size * 0.7, size * 0.1),
            Vec3::new(size * 1.45, size * 0.36, size * 0.7),
            1,
            0.0,
            face * 0.98,
        );
        tube(
            &mut p.matte,
            &[
                at + Vec3::new(-size * 1.35, size * 0.42, size * 0.62),
                at + Vec3::new(0.0, size * 0.66, size * 0.86),
                at + Vec3::new(size * 1.35, size * 0.42, size * 0.62),
            ],
            &[(0.0022, 0.0022), (0.0028, 0.0028), (0.0022, 0.0022)],
            DARK * 2.0,
        );
    }
}
