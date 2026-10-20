use super::anatomy::{storybook_part, BootShape, HumanoidBody, LimbShape};
use super::animation::ease;
use super::*;

mod rogue;

#[cfg(test)]
mod art_contract_tests {
    use super::*;

    #[test]
    fn art_contract_glowing_eyes_keep_their_lids_and_light_only_the_iris() {
        let mut glowing = Parts::default();
        soft_eyes(
            &mut glowing,
            Vec3::ZERO,
            0.04,
            0.0158,
            Vec3::splat(0.3),
            SKIN,
            true,
            Some(Accent::Danger),
        );
        assert_eq!(glowing.accent, Some(Accent::Danger));
        assert!(!glowing.glow.is_empty(), "the iris glows");
        assert!(
            !glowing.matte.is_empty(),
            "lids, lashes and the white stay solid"
        );
        let mut plain = Parts::default();
        soft_eyes(
            &mut plain,
            Vec3::ZERO,
            0.04,
            0.0145,
            Vec3::splat(0.3),
            SKIN,
            false,
            None,
        );
        assert!(plain.glow.is_empty() && plain.accent.is_none());
    }

    #[test]
    fn art_contract_structured_front_is_continuous_and_keeps_a_fitted_waist() {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Human,
                42,
                Some(Variant {
                    woman: true,
                    build,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            let leather = torso_garment(body, TorsoMaterial::Leather, true);
            let plate = torso_garment(body, TorsoMaterial::Plate, true);
            let front =
                |shape: &crate::sdf::Sculpt, x, y| shape.onto(Vec3::new(x, y, 0.6), Vec3::NEG_Z).z;
            let centre = front(&plate, 0.0, 0.345);
            let side = front(&plate, 0.073 * body.shoulders * body.build, 0.345);
            assert!(
                centre >= side - 0.01,
                "plate must span the central chest valley"
            );
            assert!(
                centre > front(&leather, 0.0, 0.345),
                "materials need different construction"
            );
            assert!(
                front(&plate, 0.0, 0.03) < centre * 0.85,
                "waist must remain fitted"
            );
            for x in [-0.08, 0.0, 0.08] {
                let at = plate.onto(Vec3::new(x, 0.345, 0.6), Vec3::NEG_Z);
                assert!(at.is_finite() && plate.distance(at).abs() < 0.001);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Building rigs.

struct Kit<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    library: &'a mut PropLibrary,
    meshes: &'a mut Assets<Mesh>,
    rig: Rig,
    key: String,
    /// Every part is drawn translucent (a phantom).
    ghost: bool,
}

impl Kit<'_, '_, '_> {
    /// A joint at a rest offset from its parent, carrying one cached part.
    fn joint(
        &mut self,
        j: J,
        parent: Entity,
        at: Vec3,
        part: &str,
        build: impl FnOnce(&mut Parts),
    ) -> Entity {
        let model = self.part(part, build);
        let e = self
            .commands
            .spawn((Transform::from_translation(at), Visibility::default()))
            .id();
        let proportion = storybook_part(self.rig.plan, j);
        self.commands
            .entity(e)
            .with_children(|p| model.attach_scaled(p, proportion));
        self.commands.entity(parent).add_child(e);
        self.rig.joints[j as usize] = Some(e);
        self.rig.rest[j as usize] = at;
        e
    }

    fn part(&mut self, part: &str, build: impl FnOnce(&mut Parts)) -> Model {
        let soft = true;
        let ghost = self.ghost;
        self.library
            .get(format!("creature/{}/{part}", self.key), self.meshes, || {
                let mut p = Parts::default();
                if soft {
                    p.soften();
                }
                build(&mut p);
                if ghost {
                    p.ghostify();
                }
                p
            })
    }

    /// Hanging cloth on its own entity, swung by the animation.
    fn drape(
        &mut self,
        parent: Entity,
        at: Transform,
        kind: Drape,
        part: &str,
        build: impl FnOnce(&mut Parts),
    ) {
        let model = self.part(part, build);
        let e = self.commands.spawn((at, Visibility::default())).id();
        self.commands
            .entity(e)
            .with_children(|p| model.attach_scaled(p, Vec3::ONE));
        self.commands.entity(parent).add_child(e);
        self.rig.drapes.push((e, kind));
    }

    fn attach(
        &mut self,
        parent: Entity,
        at: Transform,
        part: &str,
        build: impl FnOnce(&mut Parts),
    ) {
        let model = self.part(part, build);
        let e = self.commands.spawn((at, Visibility::default())).id();
        let proportion = if part == "cloak" {
            Vec3::new(1.18, 1.0, 1.0)
        } else {
            Vec3::ONE
        };
        self.commands
            .entity(e)
            .with_children(|p| model.attach_scaled(p, proportion));
        self.commands.entity(parent).add_child(e);
    }
}

/// Spawn a creature's rig under `parent` (the actor's body entity).
pub fn spawn(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    look: &Look,
) -> Rig {
    let mut kit = Kit {
        commands,
        library,
        meshes,
        rig: Rig {
            plan: look.plan,
            humanoid: (look.plan == Plan::Biped).then(|| HumanoidBody::resolve(look)),
            joints: [None; JOINTS],
            rest: [Vec3::ZERO; JOINTS],
            segments: Vec::new(),
            stride: look.stride,
            hunch: look.hunch,
            style: look.style,
            scale: look.scale,
            robe: None,
            ankle_floor: 0.081,
            legs: 1.0,
            girth: 0.2,
            drapes: Vec::new(),
        },
        key: look.key.clone(),
        ghost: look.style.ghostly(),
    };
    let lift = kit
        .commands
        .spawn((
            Transform::from_scale(Vec3::splat(look.scale)),
            Visibility::default(),
        ))
        .id();
    kit.commands.entity(parent).add_child(lift);
    kit.rig.joints[J::Lift as usize] = Some(lift);
    match look.plan {
        Plan::Biped => biped(&mut kit, lift, look),
        Plan::Bird if look.style.flies() => rogue::flier(&mut kit, lift, look),
        Plan::Bird => bird(&mut kit, lift, look),
        Plan::Quadruped if look.style.rogue_beast() => rogue::beast(&mut kit, lift, look),
        Plan::Quadruped => quadruped(&mut kit, lift, look, false),
        Plan::Rooted => rogue::flytrap(&mut kit, lift, look),
        Plan::Centaur => quadruped(&mut kit, lift, look, true),
        Plan::Serpent => serpent(&mut kit, lift, look),
        Plan::Floater => floater(&mut kit, lift, look),
        Plan::Marker => {
            let marker = kit.part("placeholder", |p| {
                p.matte.blob(
                    Vec3::Y * 0.65,
                    Vec3::new(0.36, 0.55, 0.36),
                    0,
                    0.0,
                    Vec3::new(0.38, 0.6, 0.68),
                );
            });
            kit.commands
                .entity(lift)
                .with_children(|parent| marker.attach(parent));
        }
    }
    if let Some((color, intensity)) = look.light() {
        let at = if look.plan == Plan::Floater {
            Vec3::Y * 1.1
        } else {
            Vec3::Y * 1.2
        };
        let light = kit
            .commands
            .spawn((
                PointLight {
                    color,
                    intensity,
                    range: 5.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(at),
            ))
            .id();
        kit.commands.entity(lift).add_child(light);
    }
    kit.rig
}

fn eyes(p: &mut Parts, accent: Option<Accent>, center: Vec3, spread: f32, size: f32) {
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

/// The colour of this individual's hair, by kind.
fn hair_color(l: &Look) -> Vec3 {
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
fn lock(b: &mut Builder, path: &[Vec3], width: f32, thick: f32, color: Vec3) {
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

/// A plait hanging from `top`, ending in a little tuft.
fn braid(p: &mut Parts, top: Vec3, color: Vec3) {
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

/// A face of moderate, modelled features: skull and jaw, cheekbones, a nose
/// with a bridge and wings, lidded eyes, brows, lips and ears. Women's faces
/// are softer in the jaw, with finer brows, lashes and fuller lips.
fn human_face(p: &mut Parts, l: &Look, nose: bool) {
    let fem = l.v.woman;
    let face = l.skin;
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
            Vec3::new(0.0, 0.065, 0.068),
            Vec3::new(0.037, 0.031, 0.027) * if fem { 0.9 } else { 1.0 },
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
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.088, 0.071),
            Vec3::new(if gaunt { 0.035 } else { 0.04 }, 0.028, 0.025),
        ),
        0.014,
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
    let full = if fem { 1.25 } else { 1.0 };
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.098, 0.093),
            Vec3::new(mouth_width, 0.0065 * full, 0.007),
        ),
        0.01,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.086, 0.091),
            Vec3::new(mouth_width * 0.92, 0.007 * full, 0.007),
        ),
        0.01,
    );
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
    let mouth_height = |x: f32| {
        let span = (x.abs() / mouth_width).min(1.0);
        0.091 + (0.001 + smile) * span.powi(2) + smirk * (x / mouth_width).clamp(0.0, 1.0)
    };
    // The expression follows the continuous lip surface. Its narrow seam is
    // explicit fitted geometry below, independent of the face mesh grid.
    if smile > 0.0 {
        // Cheeks lift with the smile.
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.036, 0.104, 0.082),
                    Vec3::new(0.012, 0.009, 0.009),
                ),
                0.014,
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
                if brute { 0.113 } else { 0.106 },
            ),
            0.01,
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
        face,
        fem,
        l.eyes,
    );
    // Arched brows, heavier on men, drawn down at the centre in a scowl.
    let brow_scowl = match l.style {
        Style::Hobgoblin | Style::Sentinel | Style::Orc | Style::Troll => 0.009,
        Style::Vampire | Style::Medusa | Style::Wraith => 0.005,
        Style::Zombie => -0.004,
        _ => -0.002,
    };
    let brow = hair_color(l) * 0.85;
    // Hers are fine and high-arched, as on the concept sheets.
    let (bw, bt) = if fem {
        (0.0036, 0.0026)
    } else {
        (0.009, 0.006)
    };
    for side in [-1.0f32, 1.0] {
        tube(
            &mut p.matte,
            &[
                Vec3::new(side * 0.018, 0.187 - brow_scowl, 0.101),
                Vec3::new(
                    side * 0.042,
                    0.197 + if fem { 0.007 } else { 0.0 } - brow_scowl * 0.3,
                    0.099,
                ),
                Vec3::new(side * 0.066, 0.189 + brow_scowl * 0.3, 0.09),
            ],
            &[(bw, bt), (bw, bt), (bw * 0.6, bt * 0.6)],
            brow,
        );
    }
}

/// A small, closed surface accent with a tapered cross-section. Project each
/// edge independently so a mouth stays seated on the actual lip curvature.
/// Explicit geometry resolves the seam without a sub-grid carved cavity.
fn surface_stroke(
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

/// Eyes set into the face: a white, an iris, a pupil and a glint, with an
/// upper lid that rests over the top and, for women, a line of lashes.
#[allow(clippy::too_many_arguments)]
fn soft_eyes(
    p: &mut Parts,
    center: Vec3,
    spread: f32,
    size: f32,
    iris: Vec3,
    face: Vec3,
    lashes: bool,
    glow: Option<Accent>,
) {
    if glow.is_some() {
        p.accent = glow;
    }
    // Hers are open wider beneath a lighter lid, with a larger iris.
    let (open, iris_size, lid) = if lashes {
        (0.82, 0.8, 1.02)
    } else {
        (0.68, 0.7, 0.7)
    };
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        // A glowing eye sits in a shadowed white so the light reads.
        let white = if glow.is_some() {
            face * 0.3
        } else {
            Vec3::new(0.82, 0.81, 0.76)
        };
        p.matte.blob(
            at,
            Vec3::new(size * 1.35, size * open, size * 0.65),
            0,
            0.0,
            white,
        );
        let iris_builder = if glow.is_some() {
            &mut p.glow
        } else {
            &mut p.matte
        };
        iris_builder.blob(
            at + Vec3::new(0.0, -size * 0.05, size * 0.5),
            Vec3::new(size * iris_size, size * (iris_size - 0.05), size * 0.3),
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
            Vec3::splat(size * if lashes { 0.15 } else { 0.12 }),
            0,
            0.0,
            Vec3::splat(1.6),
        );
        if lashes {
            p.matte.blob(
                at + Vec3::new(-size * 0.25, -size * 0.28, size * 0.82),
                Vec3::splat(size * 0.06),
                0,
                0.0,
                Vec3::splat(1.3),
            );
        }
        // The upper lid rests over the top of the eye; hers carries a
        // little colour, like the shadowed lids on the sheets.
        let lid_color = if lashes {
            face.lerp(Vec3::new(0.55, 0.36, 0.42), 0.18)
        } else {
            face * 0.98
        };
        p.matte.blob(
            at + Vec3::new(0.0, size * lid, size * 0.1),
            Vec3::new(size * 1.45, size * 0.36, size * 0.7),
            1,
            0.0,
            lid_color,
        );
        // The upper lid's dark edge gives every eye definition.
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
        if lashes {
            // A full upper lash line sweeping out into a winged flick, with
            // separate lashes at the outer corner and a soft lower line.
            tube(
                &mut p.matte,
                &[
                    at + Vec3::new(-side * size * 1.3, size * 0.55, size * 0.72),
                    at + Vec3::new(0.0, size * 0.82, size * 0.96),
                    at + Vec3::new(side * size * 1.45, size * 0.66, size * 0.6),
                    at + Vec3::new(side * size * 1.95, size * 1.0, size * 0.45),
                ],
                &[
                    (0.0026, 0.0026),
                    (0.0036, 0.0036),
                    (0.003, 0.003),
                    (0.0008, 0.0008),
                ],
                DARK,
            );
            for k in 0..3 {
                let along = 0.35 + 0.3 * k as f32;
                let root = at + Vec3::new(side * size * along * 1.3, size * 0.78, size * 0.9);
                p.matte.prism(
                    root,
                    root + Vec3::new(side * size * 0.45, size * 0.55, size * 0.25),
                    0.0013,
                    0.0,
                    3,
                    DARK,
                    0.0,
                );
            }
            tube(
                &mut p.matte,
                &[
                    at + Vec3::new(-side * size * 0.6, -size * 0.62, size * 0.68),
                    at + Vec3::new(side * size * 0.7, -size * 0.58, size * 0.66),
                    at + Vec3::new(side * size * 1.3, -size * 0.3, size * 0.58),
                ],
                &[(0.0012, 0.0012), (0.0016, 0.0016), (0.0008, 0.0008)],
                face * 0.55,
            );
        }
    }
}

/// A short beard along the jaw to the chin, and a moustache.
fn beard(p: &mut Parts, color: Vec3) {
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
fn crown(
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
fn hair(p: &mut Parts, l: &Look) {
    let c = hair_color(l);
    let v = l.v;
    let style = match l.style {
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
    };
    let seed = u64::from(v.hair_tone) * 31 + u64::from(style) + u64::from(v.skin) * 7;
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
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(0.0, 0.165, -0.135),
                        Vec3::new(0.0, 0.1, -0.165),
                        Vec3::new(0.0, 0.03, -0.15),
                    ],
                    0.045,
                    0.04,
                    c,
                );
            }
        }
        return;
    }
    // Hers: a side-parted crown falling over the ears to the nape.
    crown(p, &tone, 0.235, 0.12, 0.09, 0.015, 0.068, 12);
    match style {
        // Long and loose over the shoulders, locks framing the face.
        0 => {
            lock(
                &mut p.matte,
                &[
                    Vec3::new(0.0, 0.2, -0.1),
                    Vec3::new(0.0, 0.1, -0.135),
                    Vec3::new(0.0, -0.03, -0.13),
                    Vec3::new(0.0, -0.16, -0.1),
                ],
                0.125,
                0.05,
                c,
            );
            for side in [-1.0f32, 1.0] {
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(side * 0.095, 0.17, 0.03),
                        Vec3::new(side * 0.11, 0.08, 0.02),
                        Vec3::new(side * 0.105, -0.02, -0.01),
                        Vec3::new(side * 0.09, -0.11, -0.03),
                    ],
                    0.05,
                    0.034,
                    tone(40 + side as u32),
                );
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
            mass(
                p,
                Vec3::new(0.0, 0.11, -0.08),
                Vec3::new(0.105, 0.085, 0.07),
                c,
            );
        }
    }
}

/// A sculpted limb segment hanging down from its joint: a tapering core
/// from `r0` to `r1` over `len`, a muscle belly of `belly` a third of the way
/// down (pushed forward by `forward`), rounded at both ends so neighbouring
/// segments overlap without a seam.
fn limb(b: &mut Builder, len: f32, r0: f32, r1: f32, belly: f32, forward: f32, color: Vec3) {
    sculpted_limb(b, LimbShape::new(len, r0, r1, belly, forward), color);
}

pub(super) fn limb_form(shape: LimbShape) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let LimbShape {
        length: len,
        radius_top: r0,
        radius_bottom: r1,
        belly,
        belly_center,
    } = shape;
    let mut s = Sculpt::default();
    s.add(Capsule(Vec3::ZERO, Vec3::Y * -len, r0, r1), 0.0);
    if belly.x > 0.0 {
        s.add(Ellipsoid(belly_center, belly), 0.03);
    }
    s
}

fn sculpted_limb(b: &mut Builder, shape: LimbShape, color: Vec3) {
    let r = shape.radius_top.max(shape.radius_bottom).max(shape.belly.x) + 0.02;
    limb_form(shape).mesh(
        b,
        Vec3::new(-r, -shape.length - r, -r),
        Vec3::new(r, r, r),
        0.007,
        &|at, _| color * crate::sdf::Sculpt::weave(at),
    );
}

/// How far cloth stands off the body beneath it, by build.
pub(super) const SLEEVE_EASE: f32 = 0.007;
pub(super) const TROUSER_EASE: f32 = 0.008;

/// A cloth layer over a limb: the anatomy grown outward by `ease` so the
/// muscle hollows fill and it reads as fabric, gathered into soft rolls at
/// each fraction of its length in `rolls` (the crook of a joint).
pub(super) fn cloth_over(
    mut form: crate::sdf::Sculpt,
    ease: f32,
    rolls: &[(f32, f32)],
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape::Ellipsoid;
    form.inflate(ease);
    for &(y, radius) in rolls {
        form.add(
            Ellipsoid(
                Vec3::new(0.0, y, 0.004),
                Vec3::new(radius, 0.013, radius * 1.04),
            ),
            0.012,
        );
    }
    form
}

/// Trousers over a thigh or a shin, rolled at the knee.
fn trouser_leg(b: &mut Builder, shape: LimbShape, build: f32, knee: bool, color: Vec3) {
    let ease = TROUSER_EASE * build;
    let rolls: &[(f32, f32)] = if knee {
        &[(-0.035, shape.radius_top + ease + 0.004)]
    } else {
        &[(-shape.length + 0.03, shape.radius_bottom + ease + 0.005)]
    };
    let r = shape.radius_top.max(shape.radius_bottom).max(shape.belly.x) + 0.03;
    cloth_over(limb_form(shape), ease, rolls).mesh(
        b,
        Vec3::new(-r, -shape.length - r, -r),
        Vec3::new(r, r, r),
        0.007,
        &|at, _| color * crate::sdf::Sculpt::weave(at),
    );
}

/// One point of a draped garment at `t` down its length and angle `a`.
/// A free hem hangs loose: its folds deepen toward the edge, and the cloth
/// drops lower where each fold swings out, so the hem is never a level ring.
/// Short pieces such as cowls keep their neat edge.
#[allow(clippy::too_many_arguments)]
pub(super) fn drape_point(
    top: Vec2,
    bottom: Vec2,
    length: f32,
    folds: u32,
    split: f32,
    phase: f32,
    t: f32,
    a: f32,
    inner: bool,
) -> Vec3 {
    let r = top.lerp(bottom, ease(t));
    let free = (length / 0.3).clamp(0.0, 1.0);
    let wave = (a * folds as f32 + phase).cos();
    let fold = wave * (0.007 * t.powf(1.2) + 0.01 * free * t.powi(3));
    let drop = 0.013 * free * (0.5 + 0.5 * wave) * ease((t - 0.6) / 0.4);
    let thickness = if inner { 0.007 } else { 0.0 };
    let hem_split =
        split * a.cos().max(0.0).powi(if split < 0.0 { 2 } else { 14 }) * ease((t - 0.55) / 0.45);
    let opening = split.max(0.0) * a.cos().max(0.0).powi(12) * ease((t - 0.3) / 0.7);
    let side = if a < PI { 1.0 } else { -1.0 };
    Vec3::new(
        a.sin() * (r.x + fold - thickness) + side * opening,
        0.04 - (length + 0.04) * t + hem_split - drop,
        a.cos() * (r.y + fold - thickness),
    )
}

/// One articulated plate: a band from `top` down to `bottom` (heights along a
/// limb), with its radius at each edge.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lame {
    pub top: f32,
    pub bottom: f32,
    pub radius_top: f32,
    pub radius_bottom: f32,
}

/// Overlapping lames down a limb: each band's upper edge tucks inside the
/// lower edge of the one above, so the plates can slide as the limb bends
/// without opening a gap. Radii taper from `radius_top` to `radius_bottom`.
pub(super) fn lames(
    top: f32,
    count: usize,
    length: f32,
    radius_top: f32,
    radius_bottom: f32,
) -> Vec<Lame> {
    let overlap = length * 0.3;
    let radius = |y: f32| {
        let t = ((top - y) / (length * count as f32)).clamp(0.0, 1.0);
        radius_top + (radius_bottom - radius_top) * t
    };
    (0..count)
        .map(|k| {
            let upper = top - k as f32 * (length - overlap);
            let lower = upper - length;
            // Each band flares slightly at its lower edge and starts a little
            // inside the band above it.
            let tuck = if k == 0 { 0.0 } else { 0.004 };
            Lame {
                top: upper,
                bottom: lower,
                radius_top: radius(upper) - tuck,
                radius_bottom: radius(lower) + 0.003,
            }
        })
        .collect()
}

/// Draw overlapping lames as shallow metal bands with darker lower edges.
fn plate_lames(
    b: &mut Builder,
    top: f32,
    count: usize,
    length: f32,
    radius_top: f32,
    radius_bottom: f32,
) {
    for (k, lame) in lames(top, count, length, radius_top, radius_bottom)
        .into_iter()
        .enumerate()
    {
        let shade = 1.0 - 0.04 * k as f32;
        b.tube(
            &[Vec3::Y * lame.top, Vec3::Y * lame.bottom],
            &[
                Vec2::new(lame.radius_top, lame.radius_top * 0.96),
                Vec2::new(lame.radius_bottom, lame.radius_bottom * 0.96),
            ],
            16,
            &|_, _, along| STEEL * shade * if along > 0.85 { 0.82 } else { 1.0 },
        );
    }
}

/// How far each kind's leather jerkin or vest hangs free below the belt.
pub(super) fn jerkin_peplum(style: Style) -> Option<f32> {
    match style {
        Style::Hero | Style::Scout => Some(0.08),
        Style::Hobgoblin | Style::Orc => Some(0.06),
        _ => None,
    }
}

/// The skirt of each kind's tunic, robe or coat: its colour and length.
fn tunic_skirt(l: &Look, tunic: Vec3) -> (Vec3, f32) {
    match l.style {
        Style::Sentinel => (CREAM * 0.86, 0.26),
        Style::Caster => (l.cloth, 0.84),
        Style::Zombie => (tunic * 0.8, 0.2),
        Style::Vampire => (DARK, 0.3),
        Style::Orc => (LEATHER * 0.8, 0.18),
        Style::Troll => (l.cloth, 0.26),
        Style::Leprechaun => (l.cloth, 0.4),
        Style::Nymph => (l.cloth, 0.2),
        Style::Medusa => (l.cloth, 0.62),
        Style::Phantom => (l.cloth * 0.9, 0.5),
        Style::Wraith => (l.cloth, 0.66),
        Style::Hero | Style::Scout if l.v.woman => (tunic * 0.98, 0.19),
        _ => (tunic * 0.98, 0.22),
    }
}

/// A hollow garment with vertical folds, an elliptical silhouette and a
/// narrow turned hem. Its length is the hem position, not a capsule's centre.
#[allow(clippy::too_many_arguments)]
fn draped_cloth(
    b: &mut Builder,
    top: Vec2,
    bottom: Vec2,
    length: f32,
    folds: u32,
    split: f32,
    color: Vec3,
    seed: u64,
    hem_color: Option<Vec3>,
) {
    let sides = 64;
    let rows = 12;
    let phase = rand(seed + 73, 0) * 0.4;
    let point = |t: f32, a: f32, inner: bool| {
        drape_point(top, bottom, length, folds, split, phase, t, a, inner)
    };
    for row in 0..rows {
        let t0 = row as f32 / rows as f32;
        let t1 = (row + 1) as f32 / rows as f32;
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            let mid = (a0 + a1) * 0.5;
            let y = 0.04 - (length + 0.04) * (t0 + t1) * 0.5;
            let shade = 0.97 + 0.03 * (mid * folds as f32 + phase).cos();
            for inner in [false, true] {
                let inside = if inner {
                    Vec3::new(mid.sin() * 2.0, y, mid.cos() * 2.0)
                } else {
                    Vec3::Y * y
                };
                b.quad_out(
                    [
                        point(t0, a0, inner),
                        point(t0, a1, inner),
                        point(t1, a1, inner),
                        point(t1, a0, inner),
                    ],
                    inside,
                    color * shade * if inner { 0.7 } else { 1.0 },
                );
            }
        }
    }
    for t in [0.0, 1.0] {
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            b.quad_out(
                [
                    point(t, a0, false),
                    point(t, a1, false),
                    point(t, a1, true),
                    point(t, a0, true),
                ],
                Vec3::Y * (-length * 0.5),
                color * 0.83,
            );
        }
    }
    if split > 0.0 {
        // Close the turned front edges of the two panels, leaving an actual
        // opening rather than lifting a continuous bell-shaped hem.
        for a in [0.0f32, TAU] {
            for row in 0..rows {
                let t0 = row as f32 / rows as f32;
                let t1 = (row + 1) as f32 / rows as f32;
                let inside = Vec3::new(if a == 0.0 { 2.0 } else { -2.0 }, -length * 0.5, 0.0);
                b.quad_out(
                    [
                        point(t0, a, false),
                        point(t1, a, false),
                        point(t1, a, true),
                        point(t0, a, true),
                    ],
                    inside,
                    color * 0.83,
                );
            }
        }
    }
    if let Some(hem_color) = hem_color {
        // The edging shares the actual elliptical hem and its folds; a
        // circular torus floats away from the front of a fitted long robe.
        let top_t = (1.0 - 0.018 / (length + 0.04)).max(0.0);
        for column in 0..sides {
            let a0 = column as f32 * TAU / sides as f32;
            let a1 = (column + 1) as f32 * TAU / sides as f32;
            let raised = |t, angle: f32| {
                point(t, angle, false) + Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.002
            };
            b.quad_out(
                [
                    raised(top_t, a0),
                    raised(top_t, a1),
                    raised(1.0, a1),
                    raised(1.0, a0),
                ],
                Vec3::Y * -length,
                hem_color,
            );
        }
    }
}

/// A weathered stone slab: softly bevelled, its tone drifting across the
/// face, with moss settled on the upper surfaces.
fn stone(b: &mut Builder, center: Vec3, half: Vec3, color: Vec3, key: u64) {
    let moss = Vec3::new(0.38, 0.56, 0.17);
    let seed = key as f32 * 1.7;
    b.slab(center, half, Quat::IDENTITY, key, 0.06, &|dir, at| {
        let n = (at.x * 23.0 + seed).sin() * (at.z * 19.0 - seed).cos()
            + (at.y * 29.0 + at.x * 7.0).sin() * 0.6;
        let tone = color * (0.9 + 0.1 * (at.y * 9.0 + at.x * 5.0 + seed).sin());
        if (dir.y > 0.3 && n > 0.55) || (dir.y > -0.3 && n > 1.05) {
            moss
        } else {
            tone
        }
    });
}

/// A fist: a palm, four fingers curled under at the knuckles, and a thumb
/// wrapped across. `side` is +1 for the left hand, -1 for the right; `b`
/// scales it with the body.
fn fist(p: &mut Parts, at: Vec3, side: f32, b: f32, color: Vec3) {
    // The arm hangs with the palm toward the thigh (toward -side on x):
    // the knuckles run front to back, the fingers curl in toward the body,
    // and the thumb sits at the front, wrapping over them.
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.03, 0.05, 0.044) * b, 2, 0.0, color);
    for k in 0..4 {
        let z = (1.5 - k as f32) * 0.019 * b;
        let knuckle = at + Vec3::new(side * 0.004, -0.036 * b, z);
        let r = (0.0115 - k as f32 * 0.0008) * b;
        tube(
            &mut p.matte,
            &[
                knuckle,
                knuckle + Vec3::new(inward * 0.018, -0.022, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, -0.012, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, 0.008, 0.0) * b,
            ],
            &[(r, r), (r, r), (r * 0.95, r * 0.95), (r * 0.8, r * 0.8)],
            color * (0.97 - k as f32 * 0.01),
        );
    }
    let base = at + Vec3::new(inward * 0.012, -0.004, 0.03) * b;
    tube(
        &mut p.matte,
        &[
            base,
            base + Vec3::new(inward * 0.014, -0.02, 0.016) * b,
            base + Vec3::new(inward * 0.03, -0.032, 0.004) * b,
        ],
        &[
            (0.014 * b, 0.014 * b),
            (0.012 * b, 0.012 * b),
            (0.01 * b, 0.01 * b),
        ],
        color,
    );
}

/// A free hand with separated long fingers: a relaxed casting palm or an
/// undead reaching claw. Held props retain the existing closed grip.
fn open_hand(p: &mut Parts, at: Vec3, side: f32, build: f32, color: Vec3, claw: bool) {
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.027, 0.045, 0.044) * build, 2, 0.0, color);
    for finger in 0..4 {
        let spread = (1.5 - finger as f32) * 0.021;
        let short = [0.012, 0.0, 0.006, 0.023][finger];
        let root = at + Vec3::new(side * 0.003, -0.034, spread) * build;
        let radius = (0.011 - finger as f32 * 0.00065) * build;
        tube(
            &mut p.matte,
            &[
                root,
                at + Vec3::new(0.0, -0.071 + short * 0.4, spread * 1.15) * build,
                at + Vec3::new(inward * 0.012, -0.103 + short, spread * 1.35) * build,
                at + Vec3::new(
                    inward * if claw { 0.035 } else { 0.018 },
                    -0.112 + short + if claw { 0.012 } else { 0.0 },
                    spread * 1.4,
                ) * build,
            ],
            &[
                (radius, radius),
                (radius, radius),
                (radius * 0.83, radius * 0.83),
                (radius * 0.55, radius * 0.55),
            ],
            color,
        );
    }
    tube(
        &mut p.matte,
        &[
            at + Vec3::new(inward * 0.014, 0.0, 0.029) * build,
            at + Vec3::new(inward * 0.027, -0.015, 0.057) * build,
            at + Vec3::new(inward * 0.029, -0.042, 0.069) * build,
        ],
        &[
            (0.014 * build, 0.014 * build),
            (0.012 * build, 0.012 * build),
            (0.008 * build, 0.008 * build),
        ],
        color,
    );
}

/// Tufts of hair or fur: `count` tapered strands rising from around a
/// crown at `center`, swept along `sweep` and drooping at the tips. The
/// same strands make a fringe, a mane or a beard.
fn hair_tufts(
    p: &mut Parts,
    center: Vec3,
    radius: f32,
    count: u32,
    sweep: Vec3,
    color: Vec3,
    seed: u64,
) {
    let sweep = sweep.normalize_or(Vec3::Z);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU + rand(seed, k) * 0.5;
        let root = center
            + Vec3::new(
                a.cos() * radius * 0.7,
                rand(seed, k + 50) * 0.02,
                a.sin() * radius * 0.7,
            );
        let out = (Vec3::new(a.cos(), 0.1, a.sin()) * 0.6 + sweep).normalize();
        let length = radius * (0.7 + rand(seed, k + 100) * 0.4);
        let w = radius * (0.36 + rand(seed, k + 150) * 0.1);
        // Each lock lies along the head, lifting a little before it falls.
        tube(
            &mut p.matte,
            &[
                root,
                root + out * length * 0.5 + Vec3::Y * length * 0.05,
                root + out * length - Vec3::Y * length * 0.45,
            ],
            &[(w, w * 0.6), (w * 0.8, w * 0.5), (0.003, 0.003)],
            color * (0.92 + rand(seed, k + 200) * 0.16),
        );
    }
}

/// A smooth tube of one colour through `path` with elliptical `radii`.
fn tube(b: &mut Builder, path: &[Vec3], radii: &[(f32, f32)], color: Vec3) {
    let radii: Vec<Vec2> = radii.iter().map(|(x, z)| Vec2::new(*x, *z)).collect();
    b.tube(path, &radii, 10, &|_, _, _| color);
}

/// A soft leaf-shaped feather from `root`, curving outward along `out`
/// and drooping as it goes; flat, broad in the middle, pointed at the tip.
fn feather(p: &mut Parts, root: Vec3, out: Vec3, length: f32, width: f32, color: Vec3) {
    let down = Vec3::NEG_Y;
    let path = [
        root,
        root + out * length * 0.22 + down * length * 0.25,
        root + out * length * 0.34 + down * length * 0.6,
        root + out * length * 0.38 + down * length,
    ];
    let w = width * 0.5;
    let radii = [
        Vec2::new(w * 0.4, 0.006),
        Vec2::new(w, 0.01),
        Vec2::new(w * 0.62, 0.007),
        Vec2::new(0.002, 0.002),
    ];
    p.matte
        .tube(&path, &radii, 6, &|_, _, t| color * (1.0 - 0.18 * t));
}

/// Eyes with whites, a coloured iris, a pupil and a catchlight, set a
/// little forward of `center` on either side.
fn cartoon_eyes(p: &mut Parts, center: Vec3, spread: f32, size: f32, iris: Vec3) {
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        p.matte.blob(
            at,
            Vec3::new(size * 1.55, size * 1.75, size * 1.0),
            0,
            0.0,
            Vec3::new(0.95, 0.94, 0.9),
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.62),
            Vec3::new(size * 1.0, size * 1.1, size * 0.5),
            0,
            0.0,
            iris,
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.92),
            Vec3::new(size * 0.55, size * 0.62, size * 0.25),
            0,
            0.0,
            DARK,
        );
        p.matte.blob(
            at + Vec3::new(size * 0.35, size * 0.42, size * 1.05),
            Vec3::splat(size * 0.22),
            0,
            0.0,
            Vec3::splat(1.6),
        );
    }
}

/// Find the first actual front/back hit at a fixed lateral position. Whether
/// z=0 lies inside is not a reliable silhouette test on a curved or cut garment.
/// Only a genuine missed ray clamps to the garment's lateral edge; never turn
/// a few strap vertices diagonally inward while their neighbours stay in place.
fn torso_surface(form: &crate::sdf::Sculpt, x: f32, y: f32, front: bool) -> Vec3 {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let hit = |lateral: f32| -> Option<Vec3> {
        let at = |depth: f32| Vec3::new(lateral, y, 0.0) + outward * depth;
        let mut depth = 0.45;
        let mut previous = depth;
        for _ in 0..128 {
            let distance = form.distance(at(depth));
            if distance <= 0.0 {
                let (mut inside, mut outside) = (depth, previous);
                for _ in 0..10 {
                    let middle = (inside + outside) * 0.5;
                    if form.distance(at(middle)) <= 0.0 {
                        inside = middle;
                    } else {
                        outside = middle;
                    }
                }
                return Some(at((inside + outside) * 0.5));
            }
            if distance < 0.0002 {
                return Some(at(depth));
            }
            previous = depth;
            depth -= distance.clamp(0.002, 0.025);
            if depth < -0.35 {
                break;
            }
        }
        None
    };
    if let Some(surface) = hit(x) {
        return surface;
    }
    // All fitted profiles stay within the garment's vertical extent. Near
    // an armhole or waist edge, preserve height and find the last supported x.
    let Some(mut surface) = hit(0.0) else {
        return Vec3::new(x, y, 0.0);
    };
    let (mut inside, mut outside) = (0.0, x);
    for _ in 0..10 {
        let middle = (inside + outside) * 0.5;
        if let Some(at) = hit(middle) {
            inside = middle;
            surface = at;
        } else {
            outside = middle;
        }
    }
    surface
}

/// Pair a fitted surface point with its outward sculpt normal. A depth-only
/// offset becomes almost tangent on a steep chest or shoulder, leaving too
/// little clearance between the sculpt mesh and separately sampled trim.
fn torso_surface_frame(form: &crate::sdf::Sculpt, at: Vec2, front: bool) -> (Vec3, Vec3) {
    let surface = torso_surface(form, at.x, at.y, front);
    let e = 0.0015;
    let normal = Vec3::new(
        form.distance(surface + Vec3::X * e) - form.distance(surface - Vec3::X * e),
        form.distance(surface + Vec3::Y * e) - form.distance(surface - Vec3::Y * e),
        form.distance(surface + Vec3::Z * e) - form.distance(surface - Vec3::Z * e),
    )
    .normalize_or(if front { Vec3::Z } else { Vec3::NEG_Z });
    (surface, normal)
}

/// Cloth laid onto a chest or back, rather than a second closed torso that
/// intersects the first. Profile entries are height and half width.
fn fitted_panel(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    profile: &[(f32, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let at = |y: f32, x: f32| torso_surface(form, x, y, front) + outward * offset;
    let columns = (profile
        .iter()
        .map(|(_, width)| *width)
        .fold(0.0f32, f32::max)
        * 2.0
        / 0.012)
        .ceil()
        .max(4.0) as usize;
    for pair in profile.windows(2) {
        // Long chords cut through a curved bust even when their endpoints
        // touch the surface. Keep both directions close to the torso mesh's
        // resolution, including the narrow vertical trim on fitted robes.
        let rows = ((pair[1].0 - pair[0].0).abs() / 0.012).ceil().max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=rows)
            .map(|row| {
                let t = row as f32 / rows as f32;
                let (height, width) = if row == rows {
                    pair[1]
                } else {
                    (
                        pair[0].0 + (pair[1].0 - pair[0].0) * t,
                        pair[0].1 + (pair[1].1 - pair[0].1) * t,
                    )
                };
                (0..=columns)
                    .map(|column| at(height, (column as f32 / columns as f32 * 2.0 - 1.0) * width))
                    .collect()
            })
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                b.quad_out(
                    [
                        points[row][column],
                        points[row][column + 1],
                        points[row + 1][column + 1],
                        points[row + 1][column],
                    ],
                    Vec3::Y * (points[row][column].y + points[row + 1][column].y) * 0.5,
                    color,
                );
            }
        }
    }
}

/// A flat strap follows the garment across its full width. Sweeping a tube
/// through a few surface points can twist its wide axis away from the cloth
/// and interpolate back inside a fuller chest. Each row here is projected
/// after interpolation; the narrow turned edges give leather real thickness.
fn fitted_ribbon(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    path: &[(Vec2, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    if path.len() < 2 {
        return;
    }
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let fit = |at: Vec2| {
        let (surface, normal) = torso_surface_frame(form, at, front);
        (surface + normal * offset, normal)
    };
    let across = |i: usize| {
        let tangent = (path[(i + 1).min(path.len() - 1)].0 - path[i.saturating_sub(1)].0)
            .normalize_or(Vec2::Y);
        Vec2::new(tangent.y, -tangent.x)
    };
    let columns = (path.iter().map(|(_, width)| *width).fold(0.0f32, f32::max) * 2.0 / 0.012)
        .ceil()
        .max(4.0) as usize;
    let mut points: Vec<Vec<(Vec3, Vec3)>> = Vec::new();
    for (i, pair) in path.windows(2).enumerate() {
        let rows = ((pair[1].0 - pair[0].0).length() / 0.012).ceil().max(1.0) as usize;
        for row in 0..rows {
            let t = row as f32 / rows as f32;
            let center = pair[0].0.lerp(pair[1].0, t);
            let side = across(i).lerp(across(i + 1), t).normalize_or(Vec2::X);
            let width = pair[0].1 + (pair[1].1 - pair[0].1) * t;
            points.push(
                (0..=columns)
                    .map(|column| {
                        let at =
                            center + side * ((column as f32 / columns as f32 * 2.0 - 1.0) * width);
                        fit(at)
                    })
                    .collect(),
            );
        }
    }
    let (center, width) = path[path.len() - 1];
    points.push(
        (0..=columns)
            .map(|column| {
                let at = center
                    + across(path.len() - 1)
                        * ((column as f32 / columns as f32 * 2.0 - 1.0) * width);
                fit(at)
            })
            .collect(),
    );
    let thickness = 0.007;
    let inside_between = |a: (Vec3, Vec3), z: (Vec3, Vec3)| {
        (a.0 + z.0) * 0.5 - (a.1 + z.1).normalize_or(outward) * (thickness * 0.5)
    };
    for pair in points.windows(2) {
        for column in 0..columns {
            let q = [
                pair[0][column],
                pair[0][column + 1],
                pair[1][column + 1],
                pair[1][column],
            ];
            let center = q.iter().map(|point| point.0).sum::<Vec3>() * 0.25;
            let normal = q
                .iter()
                .map(|point| point.1)
                .sum::<Vec3>()
                .normalize_or(outward);
            b.quad_out(q.map(|point| point.0), center - normal * 0.1, color);
        }
        let inside = inside_between(pair[0][columns / 2], pair[1][columns / 2]);
        for column in [0, columns] {
            let (a, z) = (pair[0][column], pair[1][column]);
            b.quad_out(
                [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                inside,
                color * 0.8,
            );
        }
    }
    for (end, next) in [(0, 1), (points.len() - 1, points.len() - 2)] {
        let inside = inside_between(points[end][columns / 2], points[next][columns / 2]);
        for column in 0..columns {
            let (a, z) = (points[end][column], points[end][column + 1]);
            b.quad_out(
                [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                inside,
                color * 0.8,
            );
        }
    }
}

/// Heraldry is laid onto the tabard like embroidery. Project each small
/// triangle, so a badge follows both the chest and the hollow below it.
#[allow(clippy::too_many_arguments)]
fn fitted_emblem(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    center: Vec2,
    size: f32,
    front: bool,
    color: Vec3,
    field: Vec3,
    kind: u8,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let triangle = |b: &mut Builder, tri: [Vec2; 3], offset: f32, pigment: Vec3| {
        let steps = (tri[0]
            .distance(tri[1])
            .max(tri[1].distance(tri[2]))
            .max(tri[2].distance(tri[0]))
            * size
            / 0.012)
            .ceil()
            .max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=steps)
            .map(|row| {
                (0..=steps - row)
                    .map(|column| {
                        let uv = tri[0]
                            + (tri[1] - tri[0]) * (row as f32 / steps as f32)
                            + (tri[2] - tri[0]) * (column as f32 / steps as f32);
                        let at = center + Vec2::new(uv.x * outward.z, uv.y) * size;
                        torso_surface(form, at.x, at.y, front) + outward * offset
                    })
                    .collect()
            })
            .collect();
        for row in 0..steps {
            for column in 0..steps - row {
                let a = [
                    points[row][column],
                    points[row + 1][column],
                    points[row][column + 1],
                ];
                b.tri_out(a, a[0] - outward * 0.1, pigment);
                if column + row + 1 < steps {
                    let z = [
                        points[row + 1][column],
                        points[row + 1][column + 1],
                        points[row][column + 1],
                    ];
                    b.tri_out(z, z[0] - outward * 0.1, pigment);
                }
            }
        }
    };
    let disc = |b: &mut Builder, at: Vec2, radius: f32, offset: f32, pigment: Vec3| {
        for k in 0..32 {
            let angle = k as f32 * TAU / 32.0;
            let next = (k + 1) as f32 * TAU / 32.0;
            triangle(
                b,
                [
                    at,
                    at + Vec2::new(angle.cos(), angle.sin()) * radius,
                    at + Vec2::new(next.cos(), next.sin()) * radius,
                ],
                offset,
                pigment,
            );
        }
    };
    match kind % 4 {
        0 => {
            disc(b, Vec2::ZERO, 0.38, 0.022, color);
            for k in 0..8 {
                let angle = k as f32 * TAU / 8.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let side = Vec2::new(-dir.y, dir.x) * 0.1;
                let radius = if k % 2 == 0 { 0.8 } else { 0.66 };
                triangle(
                    b,
                    [dir * 0.42 + side, dir * radius, dir * 0.42 - side],
                    0.022,
                    color,
                );
            }
        }
        1 => {
            for k in 0..16 {
                let angle = k as f32 * TAU / 16.0;
                let next = (k + 1) as f32 * TAU / 16.0;
                let radius = |i: usize| match i % 4 {
                    0 => 0.85,
                    2 => 0.55,
                    _ => 0.22,
                };
                triangle(
                    b,
                    [
                        Vec2::ZERO,
                        Vec2::new(angle.cos(), angle.sin()) * radius(k),
                        Vec2::new(next.cos(), next.sin()) * radius(k + 1),
                    ],
                    0.022,
                    color,
                );
            }
            disc(b, Vec2::ZERO, 0.15, 0.026, field);
        }
        2 => {
            disc(b, Vec2::ZERO, 0.62, 0.022, color);
            disc(b, Vec2::new(0.0, 0.26), 0.56, 0.026, field);
        }
        _ => {
            for (x, height, width) in [(0.0, 0.85, 0.3), (-0.3, 0.45, 0.2), (0.3, 0.45, 0.2)] {
                triangle(
                    b,
                    [
                        Vec2::new(x - width, -0.6),
                        Vec2::new(x * 0.6, height),
                        Vec2::new(x + width, -0.6),
                    ],
                    0.022,
                    color,
                );
            }
            disc(b, Vec2::new(0.0, -0.45), 0.32, 0.022, color);
        }
    }
}

/// A belt or edging seated on the whole circumference of an authored torso.
fn fitted_band(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    low: f32,
    high: f32,
    offset: f32,
    color: Vec3,
) {
    let at = |y: f32, angle: f32| {
        let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
        form.onto(Vec3::Y * y + outward * 0.45, -outward) + outward * offset
    };
    let points: [[Vec3; 65]; 2] = [low, high]
        .map(|height| std::array::from_fn(|column| at(height, column as f32 * TAU / 64.0)));
    for column in 0..64 {
        b.quad_out(
            [
                points[0][column],
                points[0][column + 1],
                points[1][column + 1],
                points[1][column],
            ],
            Vec3::Y * (low + high) * 0.5,
            color,
        );
    }
}

/// The deltoid, biceps and triceps overlap the clavicle at the arm root. No
/// static shoulder ball is left on the torso when the arm rotates away.
fn biped_upper_arm(
    b: &mut Builder,
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    bare: bool,
    color: Vec3,
) {
    use crate::sdf::Sculpt;
    let fullness = if bare { 0.94 } else { 1.0 };
    let arm = upper_arm_form(side, build, length, muscle, fullness);
    arm.mesh(
        b,
        Vec3::new(-0.13 * build, -0.3 * length - 0.06 * build, -0.09 * build),
        Vec3::new(0.13 * build, 0.075 * build.max(1.0), 0.095 * build),
        0.007,
        &|at, _| color * if bare { 1.0 } else { Sculpt::weave(at) },
    );
}

/// A sleeve over the upper arm: eased off the deltoid and biceps, hanging a
/// little fuller beneath, gathered in the crook of the elbow so the bent
/// forearm never shows a gap between the two pieces of cloth.
fn biped_upper_sleeve(
    b: &mut Builder,
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    color: Vec3,
) {
    use crate::sdf::{Sculpt, Shape::*};
    let mut sleeve = upper_sleeve_form(side, build, length, muscle);
    // The underside of the sleeve falls away from the arm.
    sleeve.add(
        Ellipsoid(
            Vec3::new(side * 0.012 * build, -0.13 * length, -0.012 * build),
            Vec3::new(0.05 * build, 0.08 * length, 0.05 * build),
        ),
        0.03 * build,
    );
    sleeve.mesh(
        b,
        Vec3::new(-0.14 * build, -0.3 * length - 0.07 * build, -0.1 * build),
        Vec3::new(0.14 * build, 0.085 * build.max(1.0), 0.105 * build),
        0.007,
        &|at, _| color * Sculpt::weave(at),
    );
}

pub(super) fn upper_sleeve_form(
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
) -> crate::sdf::Sculpt {
    // Cloth softens the muscle beneath; the elbow gathers in two rolls.
    let arm = upper_arm_form(side, build, length, 1.0 + (muscle - 1.0) * 0.5, 1.0);
    cloth_over(
        arm,
        SLEEVE_EASE * build,
        &[
            (-0.245 * length, 0.056 * build),
            (-0.28 * length, 0.053 * build),
        ],
    )
}

pub(super) fn upper_arm_form(
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    fullness: f32,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let mut arm = Sculpt::default();
    arm.add(
        Capsule(
            Vec3::new(-side * 0.082 * build, 0.018, -0.005),
            Vec3::new(-side * 0.012 * build, -0.055 * length, 0.0),
            0.035 * build,
            0.059 * build * fullness,
        ),
        0.0,
    )
    .add(
        Capsule(
            Vec3::new(0.0, -0.045 * length, 0.0),
            Vec3::Y * -0.3 * length,
            0.056 * build * fullness,
            0.037 * build,
        ),
        0.025 * build,
    )
    .add(
        Ellipsoid(
            Vec3::new(-side * 0.008 * build, -0.065 * length, -0.003),
            Vec3::new(
                0.066 * build * fullness * muscle,
                0.088 * length,
                0.064 * build * muscle,
            ),
        ),
        0.024 * build,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, -0.15 * length, 0.012 * build),
            Vec3::new(
                0.053 * build * fullness * muscle,
                0.09 * length,
                0.057 * build * muscle,
            ),
        ),
        0.02 * build,
    );
    arm
}

/// A forearm has a full extensor mass below the elbow and a slim wrist;
/// bare arms keep smooth skin instead of inheriting the cloth weave.
fn biped_forearm(b: &mut Builder, build: f32, length: f32, muscle: f32, bare: bool, color: Vec3) {
    use crate::sdf::Sculpt;
    let arm = if bare {
        forearm_form(build, length, muscle)
    } else {
        forearm_sleeve_form(build, length, muscle)
    };
    arm.mesh(
        b,
        Vec3::new(-0.085 * build, -0.3 * length, -0.085 * build),
        Vec3::new(0.085 * build, 0.065 * build, 0.085 * build),
        0.007,
        &|at, _| color * if bare { 1.0 } else { Sculpt::weave(at) },
    );
}

/// A forearm sleeve eases over the extensor mass and bunches just below
/// the elbow, where it tucks under the upper sleeve's rolls.
pub(super) fn forearm_sleeve_form(build: f32, length: f32, muscle: f32) -> crate::sdf::Sculpt {
    cloth_over(
        forearm_form(build, length, 1.0 + (muscle - 1.0) * 0.5),
        SLEEVE_EASE * build * 0.85,
        &[(-0.025 * length, 0.046 * build)],
    )
}

pub(super) fn forearm_form(build: f32, length: f32, muscle: f32) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let mut arm = Sculpt::default();
    arm.add(
        // Both arm segments share this hinge-centred radius. Their
        // overlapping caps remain seated as the forearm bends, without a
        // larger second bulb forming a belt around the straight elbow.
        Capsule(
            Vec3::ZERO,
            Vec3::Y * -0.26 * length,
            0.037 * build,
            0.034 * build,
        ),
        0.0,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, -0.085 * length, 0.006 * build),
            Vec3::new(
                0.049 * build * muscle,
                0.095 * length,
                0.045 * build * muscle,
            ),
        ),
        0.016 * build,
    );
    arm
}

/// A neck widens into the collar and narrows beneath the jaw. The lower
/// station is buried below the garment opening even when the head turns.
fn biped_neck(b: &mut Builder, build: f32, slim: f32, color: Vec3) {
    let scale = build * slim;
    // Straight profiles need no spline subdivision. Finish still smooths
    // the resulting mesh normally; restoring this flag leaves later forms
    // unchanged and keeps these small transition pieces inexpensive.
    let smooth = b.smooth;
    b.smooth = false;
    tube(
        b,
        &[
            Vec3::new(0.0, -0.065, -0.008),
            Vec3::new(0.0, -0.022, -0.007),
            Vec3::new(0.0, 0.033, -0.008),
            Vec3::new(0.0, 0.085, -0.01),
        ],
        &[
            (0.056 * scale, 0.052 * scale),
            (0.05 * scale, 0.047 * scale),
            (0.045 * scale, 0.043 * scale),
            (0.045 * scale, 0.045 * scale),
        ],
        color,
    );
    b.smooth = smooth;
}

/// One sleeve or bracer follows the forearm's full upper mass and narrow
/// wrist. A thin coloured hem replaces stacked cylindrical cuff rings.
fn biped_cuff(
    b: &mut Builder,
    build: f32,
    length: f32,
    muscle: f32,
    bell: bool,
    color: Vec3,
    trim: Vec3,
) {
    let heights = if bell {
        [0.055, 0.11, 0.19, 0.25, 0.263]
    } else {
        [0.11, 0.145, 0.205, 0.25, 0.268]
    };
    let radii = if bell {
        [
            Vec2::new(0.052, 0.047),
            Vec2::new(0.057, 0.05),
            Vec2::new(0.072, 0.063),
            Vec2::new(0.084, 0.075),
            Vec2::new(0.083, 0.074),
        ]
    } else {
        [
            Vec2::new(0.053, 0.047),
            Vec2::new(0.057, 0.05),
            Vec2::new(0.049, 0.042),
            Vec2::new(0.042, 0.037),
            Vec2::new(0.043, 0.038),
        ]
    };
    let path = heights.map(|height| Vec3::Y * -height * length);
    // Follow proximal forearm fullness, then return to the authored wrist and
    // hem. Both recipes keep their final two stations and flare unchanged.
    // Bracers and cuffs are worn over the eased sleeve beneath them.
    let radii: [Vec2; 5] = std::array::from_fn(|i| {
        let influence = 1.0 - ease((heights[i] - 0.145) / (0.25 - 0.145));
        radii[i] * build * (1.0 + (muscle - 1.0) * influence) + Vec2::splat(SLEEVE_EASE * build)
    });
    let smooth = b.smooth;
    b.smooth = false;
    b.tube(&path, &radii, 12, &|_, at, along| {
        if along > 0.94 {
            trim
        } else if bell {
            color * crate::sdf::Sculpt::weave(at)
        } else {
            color
        }
    });
    b.smooth = smooth;
}

/// Thin forged shoulder plates overlap the upper arm and travel with it.
/// Open undersides and separate turned edges keep them from reading as pillows.
fn biped_pauldron(p: &mut Parts, side: f32, build: f32) {
    use crate::sdf::{Sculpt, Shape::*};
    for (center, outer, inner, cut, tint) in [
        (
            Vec3::new(-side * 0.006, -0.022, 0.0),
            Vec3::new(0.104, 0.078, 0.112),
            Vec3::new(0.092, 0.069, 0.1),
            0.073,
            1.0,
        ),
        (
            Vec3::new(side * 0.01, -0.103, 0.0),
            Vec3::new(0.086, 0.06, 0.098),
            Vec3::new(0.075, 0.053, 0.086),
            0.145,
            0.93,
        ),
    ] {
        let mut plate = Sculpt::default();
        plate
            .add(Ellipsoid(center * build, outer * build), 0.0)
            .carve(
                Ellipsoid((center - Vec3::Y * 0.012) * build, inner * build),
                0.002,
            )
            .carve(Beyond(Vec3::NEG_Y, cut * build), 0.0);
        plate.mesh(
            &mut p.metal,
            (center - outer - Vec3::splat(0.008)) * build,
            (center + outer + Vec3::splat(0.008)) * build,
            0.006,
            &|at, normal| {
                if at.y < (-cut + 0.008) * build {
                    GILT
                } else {
                    STEEL * tint * (0.97 + 0.03 * normal.z)
                }
            },
        );
    }
}

/// Construction of an existing outer garment, independent of its pigment.
#[derive(Clone, Copy)]
enum TorsoMaterial {
    Cloth,
    Leather,
    Plate,
}

fn torso_garment(
    body: HumanoidBody,
    material: TorsoMaterial,
    trim_neck: bool,
) -> crate::sdf::Sculpt {
    torso_shape(body, Some(material), trim_neck)
}

/// Body supports remain exact; garment ease is local to ribs and back so
/// the waist stays fitted and the shoulder/neck openings keep their anchors.
fn torso_shape(
    fit: HumanoidBody,
    material: Option<TorsoMaterial>,
    trim_neck: bool,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let b = fit.build;
    let (sh, w) = (fit.shoulders * b, fit.waist * b);
    let fem = fit.woman;
    let (grow, ease, bridge) = match material {
        None => (0.0, Vec2::ZERO, None),
        Some(TorsoMaterial::Cloth) => (0.010, Vec2::new(0.006, 0.004), None),
        Some(TorsoMaterial::Leather) => (
            0.014,
            Vec2::new(0.010, 0.006),
            Some(0.070 * b + 0.014 + 0.004),
        ),
        Some(TorsoMaterial::Plate) => (
            0.018,
            Vec2::new(0.012, 0.008),
            Some(0.087 * b + 0.018 + 0.008),
        ),
    };
    let g = Vec3::splat(grow);
    let rib_g = if material.is_some() {
        g + Vec3::new(ease.x, 0.0, ease.y)
    } else {
        g
    };
    let mut body = Sculpt::default();
    body.add(
        Ellipsoid(
            Vec3::new(0.0, 0.075, -0.004),
            Vec3::new(0.15 * w, 0.115, 0.108 * b) + g,
        ),
        0.0,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.29, -0.014),
            Vec3::new(0.183 * sh.max(w), 0.175, 0.119 * b) + rib_g,
        ),
        0.055,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.385, -0.047 * b),
            Vec3::new(0.18 * sh, 0.125, 0.09 * b) + rib_g,
        ),
        0.045,
    )
    .add(
        Capsule(
            Vec3::new(0.0, 0.505, -0.008),
            Vec3::new(0.0, 0.6, -0.008),
            0.059 * b + grow,
            0.053 * b + grow,
        ),
        0.035,
    );
    for side in [-1.0f32, 1.0] {
        // The clavicle slopes into the arm's moving deltoid instead
        // of leaving a separate spherical cap on the torso.
        body.add(
            Capsule(
                Vec3::new(side * 0.035 * b, 0.485, -0.005),
                Vec3::new(side * 0.195 * sh, 0.465, -0.005),
                0.038 * b + grow,
                0.034 * b + grow,
            ),
            0.035,
        );
        if fem {
            body.add(
                Ellipsoid(
                    Vec3::new(side * 0.073 * sh, 0.345, 0.087 * b),
                    Vec3::new(0.094 * b, 0.083, 0.087 * b) + g,
                ),
                0.038,
            );
        }
    }
    if !fem {
        // One broad, shallow pectoral plane joins both shoulders to
        // the rib cage. Paired forward lobes read as breast cups,
        // especially when the fitted leather follows their shape.
        body.add(
            Ellipsoid(
                Vec3::new(0.0, 0.39, -0.006 * b),
                Vec3::new(0.185 * sh, 0.11, 0.095 * b) + g,
            ),
            0.04,
        );
    }
    // Structured fronts span the chest valley while the original supports
    // retain the upper and lateral bust. A single replacement ellipsoid
    // would not enclose those offset supports without flattening the shape.
    if fem {
        if let Some(radius) = bridge {
            body.add(
                Capsule(
                    Vec3::new(-0.073 * sh, 0.345, 0.087 * b),
                    Vec3::new(0.073 * sh, 0.345, 0.087 * b),
                    radius,
                    radius,
                ),
                0.02,
            );
        }
    }
    if trim_neck {
        // The tunic ends at its fitted opening rather than climbing
        // inside the moving skin neck as a second thick cylinder.
        body.carve(Beyond(Vec3::Y, 0.552), 0.004);
    }
    body
}

/// Upright figures: people, the dead, constructs and the yeti.
///
/// The body is shaped rather than stacked: a chest that narrows to the
/// waist, shoulders, a skirt over the hips, tapered limbs with knees and
/// elbows, calves, hands with thumbs, cuffed boots. Clothing is layered in
/// separate colours so a figure reads at a glance even from above.
fn biped(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let style = l.style;
    let body = kit
        .rig
        .humanoid
        .expect("biped anatomy resolved before generation");
    let fem = body.woman;
    let b = body.build;
    let (shoulders, waist, hips_w) = (body.shoulders, body.waist, body.hips_width);
    let arm_len = body.arm_length;
    // Footwear is an outfit choice fitted to the resolved wearer.
    let boot_shape = BootShape::fitted(
        b,
        if fem && matches!(style, Style::Hero | Style::Scout) {
            -0.075
        } else {
            -0.24
        },
    );
    let blocky = style == Style::Guardian;
    let furry = style == Style::Yeti;
    // The colour of each layer.
    let tunic = match style {
        Style::Orc | Style::Troll => LEATHER * 0.9,
        // A purple waistcoat under the open green coat.
        Style::Leprechaun => Vec3::new(0.34, 0.16, 0.36),
        _ => l.cloth,
    };
    let trousers = [
        Vec3::new(0.25, 0.17, 0.12),
        Vec3::new(0.2, 0.22, 0.16),
        Vec3::new(0.34, 0.28, 0.2),
        Vec3::new(0.16, 0.17, 0.22),
        Vec3::new(0.36, 0.2, 0.14),
    ];
    let pants = match style {
        Style::Hero => trousers[l.v.shade as usize],
        Style::Sentinel => IRON,
        Style::Guardian => l.skin,
        Style::Yeti => l.cloth,
        Style::Zombie => Vec3::new(0.3, 0.26, 0.2),
        Style::Vampire => DARK * 2.0,
        Style::Scout => Vec3::new(0.22, 0.2, 0.15),
        Style::Orc => Vec3::new(0.3, 0.2, 0.13),
        Style::Troll | Style::Nymph => l.skin,
        Style::Leprechaun => Vec3::new(0.2, 0.17, 0.15),
        Style::Medusa => Vec3::new(0.16, 0.1, 0.1),
        Style::Phantom => Vec3::new(0.2, 0.18, 0.24),
        Style::Wraith => DARK * 2.6,
        _ => l.cloth * 0.55 + LEATHER * 0.45,
    };
    // Bare-limbed kinds show skin where others wear sleeves and leggings.
    let bare_arms = matches!(
        style,
        Style::Hobgoblin
            | Style::Zombie
            | Style::Orc
            | Style::Troll
            | Style::Nymph
            | Style::Medusa
            | Style::Phantom
    );
    let barefoot = matches!(style, Style::Troll | Style::Nymph);
    // Cloth trousers stand off the leg; skin, stockings and mail stay fitted.
    let trousered = matches!(
        style,
        Style::Hero
            | Style::Scout
            | Style::Common
            | Style::Vampire
            | Style::Hobgoblin
            | Style::Zombie
            | Style::Orc
            | Style::Leprechaun
            | Style::Phantom
            | Style::Wraith
    );
    let boot = match style {
        Style::Guardian => l.skin * 0.85,
        Style::Yeti => l.skin,
        Style::Sentinel => STEEL,
        Style::Zombie => l.skin * 0.8,
        Style::Vampire => DARK,
        Style::Troll => l.skin * 0.88,
        Style::Nymph => l.skin,
        Style::Leprechaun => LEATHER * 0.9,
        Style::Medusa => Vec3::new(0.4, 0.24, 0.14),
        Style::Wraith => Vec3::new(0.2, 0.16, 0.14),
        _ => LEATHER * 0.75,
    };
    let sleeve = match style {
        Style::Guardian | Style::Hobgoblin => l.skin,
        Style::Yeti => l.cloth,
        Style::Sentinel => STEEL,
        Style::Zombie => l.skin,
        Style::Vampire => Vec3::new(0.22, 0.12, 0.17),
        _ if bare_arms => l.skin,
        Style::Leprechaun => l.cloth,
        _ => tunic,
    };
    let hand_color = match style {
        Style::Yeti => l.skin,
        Style::Sentinel => STEEL,
        Style::Hero | Style::Scout => LEATHER,
        _ => l.skin,
    };
    let hips = kit.joint(J::Hips, lift, body.hips, "hips", |p| {
        if blocky {
            stone(
                &mut p.matte,
                Vec3::ZERO,
                Vec3::new(0.2, 0.1, 0.14),
                l.skin,
                1,
            );
            return;
        }
        p.matte
            .blob(Vec3::ZERO, body.pelvis_radii(), 3, 0.05, pants);
        if furry {
            p.matte.blob(
                Vec3::new(0.0, -0.05, 0.0),
                Vec3::new(0.24, 0.16, 0.2),
                6,
                0.25,
                l.cloth,
            );
            return;
        }
        // The tunic's own skirt is a separate drape (see below).
        if style == Style::Zombie {
            for k in 0..5 {
                let a = k as f32 * 1.3;
                let at = Vec3::new(a.cos() * 0.2 * body.profile.hips, -0.2, a.sin() * 0.2);
                p.matte.prism(
                    at,
                    at + Vec3::new(a.cos() * 0.03 * body.profile.hips, -0.1, a.sin() * 0.03),
                    0.04,
                    0.0,
                    3,
                    tunic * 0.7,
                    0.0,
                );
            }
        }
        if style == Style::Sentinel {
            // Long tabard panels front and back, gilt-edged, falling past the belt.
            for (z, lean) in [(0.19f32, -0.07f32), (-0.19, 0.07)] {
                let down = Vec3::new(0.0, 1.0, lean).normalize();
                let out = Vec3::Z * z.signum();
                p.metal.extrude(
                    &[
                        Vec2::new(-0.11 * body.profile.hips, 0.0),
                        Vec2::new(0.11 * body.profile.hips, 0.0),
                        Vec2::new(0.125 * body.profile.hips, -0.4),
                        Vec2::new(-0.125 * body.profile.hips, -0.4),
                    ],
                    Vec3::new(0.0, 0.05, z),
                    Vec3::X,
                    down,
                    0.012,
                    GILT,
                );
                p.matte.extrude(
                    &[
                        Vec2::new(-0.097 * body.profile.hips, 0.0),
                        Vec2::new(0.097 * body.profile.hips, 0.0),
                        Vec2::new(0.11 * body.profile.hips, -0.38),
                        Vec2::new(-0.11 * body.profile.hips, -0.38),
                    ],
                    Vec3::new(0.0, 0.05, z) + out * 0.01,
                    Vec3::X,
                    down,
                    0.012,
                    l.cloth,
                );
            }
        }
        let belt = super::robe::belt_radii(b, waist);
        tube(
            &mut p.satin,
            &[Vec3::Y * 0.045, Vec3::Y * 0.095],
            &[(belt.x, belt.y), (belt.x, belt.y)],
            LEATHER * 0.8,
        );
        p.metal.cuboid(
            Vec3::new(0.0, 0.07, belt.y + 0.008),
            Vec3::new(0.035, 0.03, 0.012),
            Quat::IDENTITY,
            Vec3::new(0.74, 0.5, 0.26),
        );
        if style == Style::Caster {
            // The knot and its surface-bound tails share the garment/belt
            // frame. Fit depth to that belt while retaining the authored knot.
            let x = 0.06;
            let front = belt.y * (1.0 - (x / belt.x).powi(2)).max(0.0).sqrt();
            p.matte.blob(
                Vec3::new(x, 0.11, front + 0.01),
                Vec3::new(0.035, 0.03, 0.02),
                1,
                0.0,
                l.trim * 0.8,
            );
        }
        if matches!(style, Style::Hero | Style::Scout) {
            p.satin.cuboid(
                Vec3::new(0.15 * b * body.profile.hips, -0.02, 0.1),
                Vec3::new(0.035, 0.05, 0.025),
                Quat::from_rotation_y(0.5),
                LEATHER,
            );
        }
    });
    // The tunic's skirt hangs from the pelvis on its own, free to sway.
    if !blocky && !furry && style != Style::Caster {
        let (skirt_color, skirt_len) = tunic_skirt(&l, tunic);
        let flare = if style == Style::Sentinel {
            0.185
        } else {
            0.208 + skirt_len * 0.045
        };
        let top = Vec2::new(0.18 * b * hips_w, 0.135 * b);
        let bottom = Vec2::new(flare * b * hips_w, flare * 0.74 * b);
        kit.drape(
            hips,
            Transform::IDENTITY,
            Drape::Skirt(skirt_len),
            "skirt",
            |p| {
                draped_cloth(
                    &mut p.matte,
                    top,
                    bottom,
                    skirt_len,
                    if skirt_len > 0.5 { 12 } else { 8 },
                    match style {
                        Style::Hero | Style::Scout => 0.055,
                        // Coat tails and a robe-skirt part at the front.
                        Style::Leprechaun => 0.07,
                        Style::Medusa => 0.05,
                        _ => 0.0,
                    },
                    skirt_color,
                    u64::from(l.v.shade),
                    matches!(style, Style::Medusa | Style::Leprechaun).then_some(l.trim),
                );
            },
        );
    }
    // A jerkin's free region: below the belt its leather is no longer
    // supported by the body and hangs as a short flared peplum over the tunic.
    if let Some(length) = jerkin_peplum(style) {
        let leather = LEATHER * if style == Style::Scout { 0.85 } else { 0.9 };
        let top = Vec2::new(0.19 * b * hips_w, 0.143 * b);
        let bottom = Vec2::new(0.215 * b * hips_w, 0.162 * b);
        kit.drape(
            hips,
            Transform::from_xyz(0.0, 0.035, 0.0),
            Drape::Skirt(length),
            "peplum",
            |p| {
                draped_cloth(
                    &mut p.satin,
                    top,
                    bottom,
                    length,
                    6,
                    if matches!(style, Style::Hero | Style::Scout) {
                        0.06
                    } else {
                        0.0
                    },
                    leather,
                    u64::from(l.v.pattern),
                    Some(leather * 0.7),
                );
            },
        );
    }
    if style == Style::Caster {
        let sashes = [(0.05f32, 0.32f32), (0.085, 0.26)].map(|(dx, end)| super::robe::SashTail {
            path: [
                Vec2::new(dx, 0.10),
                Vec2::new(dx + 0.01, -0.04),
                Vec2::new(dx + 0.015, 0.06 - end),
            ],
            radii: [
                Vec2::new(0.022, 0.006),
                Vec2::new(0.024, 0.006),
                Vec2::new(0.026, 0.006),
            ],
            color: l.trim * 0.8,
        });
        kit.rig.robe = Some(super::robe::spawn(
            kit.commands,
            kit.library,
            kit.meshes,
            hips,
            &l,
            body,
            boot_shape,
            &sashes,
        ));
    }
    let torso = kit.joint(J::Torso, hips, body.torso, "torso", |p| {
        match style {
            Style::Guardian => {
                stone(
                    &mut p.matte,
                    Vec3::Y * 0.3,
                    Vec3::new(0.27, 0.27, 0.17),
                    l.skin,
                    2,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.5, 0.0),
                    Vec3::new(0.32, 0.1, 0.19),
                    l.skin * 1.08,
                    3,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.1, 0.0),
                    Vec3::new(0.2, 0.08, 0.13),
                    l.skin * 0.9,
                    4,
                );
                p.accent = Some(Accent::Teal);
                p.glow.torus(
                    Vec3::new(0.0, 0.3, 0.172),
                    Vec3::Z,
                    0.07,
                    0.012,
                    14,
                    3,
                    Vec3::ONE,
                );
                p.glow.cuboid(
                    Vec3::new(0.0, 0.3, 0.172),
                    Vec3::new(0.012, 0.06, 0.004),
                    Quat::IDENTITY,
                    Vec3::ONE,
                );
                return;
            }
            Style::Yeti => {
                p.matte
                    .blob(Vec3::Y * 0.28, Vec3::new(0.3, 0.34, 0.24), 9, 0.08, l.cloth);
                p.matte.blob(
                    Vec3::new(0.0, 0.28, 0.12),
                    Vec3::new(0.18, 0.2, 0.14),
                    4,
                    0.05,
                    l.trim,
                );
                p.matte.blob(
                    Vec3::new(0.0, 0.5, -0.05),
                    Vec3::new(0.26, 0.12, 0.2),
                    7,
                    0.1,
                    l.cloth,
                );
                // A mantle of long fur over the shoulders and down the chest.
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.56, 0.0),
                    0.24,
                    14,
                    Vec3::new(0.0, -1.0, 0.0),
                    l.cloth,
                    31,
                );
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.42, 0.12),
                    0.14,
                    7,
                    Vec3::new(0.0, -1.0, 0.6),
                    l.cloth * 0.96,
                    37,
                );
                return;
            }
            _ => {}
        }
        // The torso is sculpted as one form: waist, ribcage, a chest that
        // broadens into the shoulders, and deltoids that round over the top
        // of the arms, all blended so no seam shows. Outer garments use the
        // same supports with material-specific room and chest construction.
        use crate::sdf::{Sculpt, Shape::*};
        let (sh, w) = (shoulders * b, waist * b);
        let low_neck = fem
            && matches!(
                style,
                Style::Hero
                    | Style::Scout
                    | Style::Common
                    | Style::Vampire
                    | Style::Leprechaun
                    | Style::Medusa
                    | Style::Nymph
                    | Style::Phantom
                    | Style::Wraith
                    | Style::Orc
                    | Style::Troll
            );
        let trim_neck = !matches!(
            style,
            Style::Sentinel | Style::Hobgoblin | Style::Zombie | Style::Orc | Style::Troll
        );
        let body_shape = torso_shape(body, None, trim_neck);
        let bare_shoulders = bare_arms;
        body_shape.mesh(
            &mut p.matte,
            Vec3::new(-0.38, -0.06, -0.21) * b.max(1.0),
            Vec3::new(0.38, 0.64, 0.24) * b.max(1.0),
            0.0085,
            &|at, normal| {
                let bare = bare_shoulders && (at.x.abs() > 0.16 * sh || at.y > 0.51);
                // Women's bodices open in a low, shaped neckline over the
                // collarbones and upper chest, as on the concept sheets.
                let decollete = low_neck
                    && normal.z > 0.0
                    && at.y > 0.4
                    && at.x.abs() < 0.045 * sh + (at.y - 0.4) * 0.95;
                if decollete {
                    return l.skin;
                }
                if style == Style::Zombie && normal.z > 0.35 {
                    let tear = ((at.x - 0.055 * b) / (0.068 * b)).powi(2)
                        + ((at.y - 0.25) / 0.105).powi(2);
                    let wound =
                        ((at.x + 0.1 * b) / (0.043 * b)).powi(2) + ((at.y - 0.38) / 0.055).powi(2);
                    if tear < 1.0 + 0.12 * (at.y * 93.0).sin() {
                        return l.skin * 0.85;
                    }
                    if wound < 1.0 {
                        return Vec3::new(0.35, 0.12, 0.1);
                    }
                }
                if bare {
                    l.skin
                } else {
                    tunic * Sculpt::weave(at)
                }
            },
        );
        if low_neck {
            // A sewn edge finishes the neckline on both sides.
            let edge = if style == Style::Medusa {
                l.trim
            } else {
                tunic * 0.72
            };
            for side in [-1.0f32, 1.0] {
                let at = |y: f32| Vec2::new(side * (0.045 * sh + (y - 0.4) * 0.95), y);
                fitted_ribbon(
                    &mut p.matte,
                    &body_shape,
                    &[
                        (at(0.395), 0.011),
                        (at(0.44), 0.012),
                        (at(0.49), 0.012),
                        (at(0.53), 0.011),
                    ],
                    true,
                    0.004,
                    edge,
                );
            }
        }
        if matches!(style, Style::Medusa | Style::Phantom | Style::Nymph) {
            // A sleeveless bodice ends in sewn edges where the shoulders
            // and arms are bare, front and back.
            let edge = if style == Style::Medusa {
                l.trim
            } else {
                tunic * 0.72
            };
            for front in [true, false] {
                for side in [-1.0f32, 1.0] {
                    let x = side * 0.16 * sh;
                    fitted_ribbon(
                        &mut p.matte,
                        &body_shape,
                        &[
                            (Vec2::new(x * 0.98, 0.1), 0.011),
                            (Vec2::new(x, 0.3), 0.012),
                            (Vec2::new(x, 0.5), 0.011),
                        ],
                        front,
                        0.004,
                        edge,
                    );
                }
            }
        }
        // A fitted outer garment: a jerkin, vest or coat over the tunic.
        let outer = match style {
            Style::Hero => Some((LEATHER, 0.465, true)),
            Style::Scout => Some((LEATHER * 0.85, 0.465, true)),
            Style::Hobgoblin => Some((LEATHER * 0.9, 0.43, true)),
            Style::Vampire => Some((Vec3::new(0.32, 0.05, 0.07), 0.47, false)),
            Style::Orc => Some((LEATHER * 0.95, 0.44, true)),
            Style::Leprechaun => Some((l.cloth, 0.47, false)),
            Style::Wraith => Some((l.cloth * 1.15, 0.48, false)),
            _ => None,
        };
        let shell = outer.map(|(color, top, sleeveless)| {
            let material = if matches!(style, Style::Vampire | Style::Leprechaun | Style::Wraith) {
                TorsoMaterial::Cloth
            } else {
                TorsoMaterial::Leather
            };
            let mut shell = torso_garment(body, material, trim_neck);
            shell
                .carve(Beyond(Vec3::Y, top), 0.01)
                .carve(Beyond(-Vec3::Y, 0.0), 0.0);
            if sleeveless {
                for side in [-1.0f32, 1.0] {
                    shell.carve(Beyond(Vec3::X * side, 0.185 * sh), 0.008);
                }
                if matches!(style, Style::Hero | Style::Scout | Style::Orc) {
                    // A shaped neckline leaves the fitted upper sides high
                    // while the centre opens over the under-tunic; hers
                    // plunges lower to frame the open bodice beneath.
                    let (depth, reach) = if fem { (0.0, 0.125) } else { (0.055, 0.105) };
                    shell.carve(
                        Ellipsoid(
                            Vec3::new(0.0, top + depth, 0.135 * b),
                            Vec3::new(0.11 * sh, reach, 0.15 * b),
                        ),
                        0.006,
                    );
                }
            }
            if fem && matches!(style, Style::Vampire | Style::Wraith) {
                // Her coat falls open from the collar in a deep V.
                shell.carve(
                    Ellipsoid(
                        Vec3::new(0.0, top + 0.01, 0.15 * b),
                        Vec3::new(0.085 * sh, 0.15, 0.13 * b),
                    ),
                    0.006,
                );
            }
            if style == Style::Leprechaun {
                // The coat hangs open over the waistcoat.
                shell.carve(
                    Ellipsoid(
                        Vec3::new(0.0, 0.2, 0.2 * b),
                        Vec3::new(0.075 * sh, 0.3, 0.12 * b),
                    ),
                    0.008,
                );
            }
            let c = color;
            shell.mesh(
                &mut p.satin,
                Vec3::new(-0.36, -0.03, -0.2) * b.max(1.0),
                Vec3::new(0.36, top + 0.03, 0.22) * b.max(1.0),
                0.0075,
                &move |at, n| {
                    // Seams darken the front opening and the edges.
                    let seam = if at.x.abs() < 0.006 && n.z > 0.5 {
                        0.7
                    } else {
                        1.0
                    };
                    c * seam * (1.0 + 0.03 * (at.y * 57.0 + at.x * 31.0).sin())
                },
            );
            shell
        });
        // Where something sits on the front (or back) of the outer garment.
        let garment = shell.as_ref().unwrap_or(&body_shape);
        let on = |x: f32, y: f32, front: bool| -> Vec3 { torso_surface(garment, x, y, front) };
        if style == Style::Sentinel {
            // The separate gorget belongs to the articulated plate armour.
            p.matte
                .torus(Vec3::Y * 0.555, Vec3::Y, 0.07, 0.022, 12, 4, l.trim);
        } else if !bare_shoulders {
            fitted_band(&mut p.matte, &body_shape, 0.535, 0.552, 0.0015, l.trim);
        }
        // A soft cowl or scarf gathered round the neck and draped over the shoulders.
        let cowl = match style {
            Style::Hero => Some((tunic * 0.75, false)),
            Style::Caster => Some((l.cloth * 0.85, false)),
            Style::Scout => Some((Vec3::new(0.3, 0.42, 0.22), true)),
            // The orc's red scarf, the phantom's torn shawl.
            Style::Orc => Some((l.trim, true)),
            Style::Phantom => Some((l.cloth * 0.85, true)),
            _ => None,
        };
        if let Some((color, tail)) = cowl {
            p.set_transform(Affine3A::from_translation(Vec3::new(0.0, 0.515, -0.012)));
            draped_cloth(
                &mut p.matte,
                Vec2::new(0.087, 0.078) * b,
                Vec2::new(0.15, 0.118) * b,
                0.045,
                8,
                -0.025,
                color,
                u64::from(l.v.pattern),
                None,
            );
            p.set_transform(Affine3A::IDENTITY);
            if tail {
                fitted_ribbon(
                    &mut p.matte,
                    garment,
                    &[
                        (Vec2::new(0.07 * b, 0.455), 0.027 * b),
                        (Vec2::new(0.075 * b, 0.36), 0.028 * b),
                        (Vec2::new(0.09 * b, 0.26), 0.022 * b),
                    ],
                    true,
                    0.014,
                    color,
                );
            }
        }

        match style {
            Style::Hero => {
                // Lacing up the front of the jerkin and a strap across it.
                for k in 0..4 {
                    let height = 0.1 + k as f32 * 0.08;
                    fitted_ribbon(
                        &mut p.matte,
                        garment,
                        &[
                            (Vec2::new(-0.021 * b, height - 0.014), 0.004),
                            (Vec2::new(0.021 * b, height + 0.014), 0.004),
                        ],
                        true,
                        0.012,
                        BONE,
                    );
                }
                let strap = [
                    (Vec2::new(0.16 * sh, 0.43), 0.024 * b),
                    (Vec2::new(0.07 * b, 0.33), 0.024 * b),
                    (Vec2::new(-0.06 * b, 0.2), 0.024 * b),
                    (Vec2::new(-0.13 * w, 0.025), 0.022 * b),
                ];
                fitted_ribbon(&mut p.satin, garment, &strap, true, 0.017, LEATHER * 0.7);
                let (buckle, normal) = torso_surface_frame(garment, Vec2::new(0.04 * b, 0.3), true);
                p.metal.blob(
                    buckle + normal * 0.024,
                    Vec3::new(0.02, 0.02, 0.008),
                    0,
                    0.0,
                    Vec3::new(0.74, 0.5, 0.26),
                );
            }
            Style::Sentinel => {
                // One fitted cuirass follows the rib cage and waist. Cloth
                // is an open panel laid on its surface, never a second torso
                // intersecting the metal or several circular gold bands.
                let mut cuirass = torso_garment(body, TorsoMaterial::Plate, trim_neck);
                cuirass
                    .carve(Beyond(Vec3::Y, 0.525), 0.004)
                    .carve(Beyond(Vec3::NEG_Y, -0.012), 0.0);
                cuirass.mesh(
                    &mut p.metal,
                    Vec3::new(-0.34, -0.025, -0.23) * b.max(1.0),
                    Vec3::new(0.34, 0.55, 0.255) * b.max(1.0),
                    0.008,
                    &|_, normal| STEEL * (0.92 + 0.08 * normal.z.abs()),
                );
                fitted_band(&mut p.metal, &cuirass, 0.018, 0.034, 0.004, GILT);
                for front in [true, false] {
                    let edge = [
                        (0.035, 0.112 * w),
                        (0.16, 0.143 * sh),
                        (0.31, 0.166 * sh),
                        (0.435, 0.149 * sh),
                        (0.512, 0.075 * b),
                    ];
                    let cloth = [
                        (0.048, 0.1 * w),
                        (0.16, 0.13 * sh),
                        (0.31, 0.153 * sh),
                        (0.435, 0.136 * sh),
                        (0.501, 0.064 * b),
                    ];
                    fitted_panel(&mut p.matte, &cuirass, &edge, front, 0.009, GILT);
                    fitted_panel(&mut p.matte, &cuirass, &cloth, front, 0.014, l.cloth);
                    fitted_emblem(
                        &mut p.matte,
                        &cuirass,
                        Vec2::new(0.0, 0.31),
                        0.075 * b,
                        front,
                        CREAM,
                        l.cloth,
                        l.v.pattern,
                    );
                }
            }
            Style::Caster => {
                fitted_panel(
                    &mut p.matte,
                    &body_shape,
                    &[(0.02, 0.025 * b), (0.25, 0.031 * b), (0.49, 0.025 * b)],
                    true,
                    0.01,
                    l.trim,
                );
                fitted_band(&mut p.matte, &body_shape, 0.035, 0.07, 0.006, l.trim * 0.8);
            }
            Style::Scout => {
                // A baldric across the jerkin.
                let strap = [
                    (Vec2::new(-0.16 * sh, 0.43), 0.026 * b),
                    (Vec2::new(-0.06 * b, 0.32), 0.026 * b),
                    (Vec2::new(0.07 * b, 0.19), 0.026 * b),
                    (Vec2::new(0.13 * w, 0.025), 0.022 * b),
                ];
                fitted_ribbon(&mut p.satin, garment, &strap, true, 0.017, LEATHER);
            }
            Style::Hobgoblin => {
                // A riveted leather vest on shoulder straps.
                for side in [-1.0f32, 1.0] {
                    let (f, k) = (on(side * 0.11, 0.41, true), on(side * 0.11, 0.41, false));
                    tube(
                        &mut p.satin,
                        &[
                            f,
                            Vec3::new(side * 0.13, 0.53, 0.05),
                            Vec3::new(side * 0.13, 0.53, -0.06),
                            k,
                        ],
                        &[(0.03, 0.01); 4],
                        LEATHER * 0.75,
                    );
                }
                for k in 0..3 {
                    p.metal.blob(
                        on(-0.1 + k as f32 * 0.1, 0.16, true),
                        Vec3::splat(0.012),
                        0,
                        0.0,
                        IRON,
                    );
                }
            }
            Style::Zombie => {
                for k in 0..3 {
                    p.matte.cuboid(
                        on(0.055 * b, 0.2 + k as f32 * 0.04, true) + Vec3::Z * 0.003,
                        Vec3::new(0.04 * b, 0.005, 0.004),
                        Quat::IDENTITY,
                        BONE * 0.8,
                    );
                }
            }
            Style::Vampire => {
                // A stand-up collar framing the head from behind.
                // Plum shoulder pads remain distinct from the dark cape.
                for side in [-1.0f32, 1.0] {
                    p.matte.blob(
                        Vec3::new(side * 0.2 * b * body.profile.shoulders, 0.47, 0.0),
                        Vec3::new(0.085, 0.045, 0.09),
                        2,
                        0.0,
                        Vec3::new(0.16, 0.08, 0.12),
                    );
                }
                // A high collar, red-lined, rising behind the head and folding open at the front.
                tube(
                    &mut p.matte,
                    &[
                        Vec3::new(0.0, 0.5, -0.05),
                        Vec3::new(0.0, 0.58, -0.08),
                        Vec3::new(0.0, 0.67, -0.11),
                    ],
                    &[(0.11, 0.085), (0.13, 0.095), (0.155, 0.105)],
                    l.trim,
                );
                // A narrow white cravat and two rows of gold buttons.
                fitted_ribbon(
                    &mut p.matte,
                    garment,
                    &[
                        (Vec2::new(0.0, 0.46), 0.024 * b),
                        (Vec2::new(0.0, 0.36), 0.02 * b),
                        (Vec2::new(0.0, 0.26), 0.012 * b),
                    ],
                    true,
                    0.01,
                    Vec3::new(0.88, 0.85, 0.8),
                );
                for side in [-1.0f32, 1.0] {
                    for k in 0..3 {
                        p.metal.blob(
                            on(side * 0.055, 0.13 + k as f32 * 0.08, true),
                            Vec3::splat(0.011),
                            0,
                            0.0,
                            Vec3::new(0.9, 0.75, 0.35),
                        );
                    }
                }
            }
            _ => rogue::torso_details(p, &l, body, garment, &body_shape),
        }
    });
    kit.joint(J::Head, torso, body.head, "head", |p| {
        let neck = if furry { l.cloth } else { l.skin };
        let slim = if fem { 0.85 } else { 1.0 };
        if blocky || furry || style == Style::Sentinel {
            p.matte.prism(
                Vec3::Y * -0.03,
                Vec3::Y * 0.08,
                0.06 * b * slim,
                0.052 * b * slim,
                7,
                neck,
                0.0,
            );
        } else {
            biped_neck(&mut p.matte, b, slim, neck);
        }
        match style {
            Style::Guardian => {
                stone(
                    &mut p.matte,
                    Vec3::Y * 0.15,
                    Vec3::new(0.13, 0.12, 0.13),
                    l.skin,
                    5,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.2, 0.115),
                    Vec3::new(0.12, 0.03, 0.03),
                    l.skin * 0.8,
                    6,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.052, 0.11),
                    Vec3::new(0.08, 0.022, 0.03),
                    l.skin * 0.75,
                    7,
                );
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.13), 0.05, 0.022);
                // The chin ends below the smile instead of hiding its middle.
                // Wear chips inward from the face slab's .13 front plane.
                let smile: [Vec2; 25] = std::array::from_fn(|i| {
                    let u = i as f32 / 24.0 * 2.0 - 1.0;
                    Vec2::new(u * 0.052, 0.085 + u * u * 0.01)
                });
                surface_stroke(
                    &mut p.matte,
                    &smile,
                    0.003,
                    &|at| (Vec3::new(at.x, at.y, 0.13), Vec3::Z),
                    DARK * 2.0,
                );
            }
            Style::Yeti => {
                p.matte.blob(
                    Vec3::new(0.0, 0.14, 0.0),
                    Vec3::new(0.15, 0.15, 0.14),
                    6,
                    0.06,
                    l.cloth,
                );
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.24, -0.02),
                    0.12,
                    9,
                    Vec3::new(0.0, -0.4, -1.0),
                    l.cloth,
                    41,
                );
                // One dark ape face joins brow, flat nose and muzzle. The
                // fitted mouth is independent of the surrounding fur shell.
                use crate::sdf::{Sculpt, Shape::Ellipsoid};
                let mut face = Sculpt::default();
                face.add(
                    Ellipsoid(Vec3::new(0.0, 0.12, 0.09), Vec3::new(0.1, 0.1, 0.07)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.175, 0.12), Vec3::new(0.09, 0.025, 0.03)),
                    0.018,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.069, 0.132), Vec3::new(0.061, 0.035, 0.041)),
                    0.018,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.119, 0.16), Vec3::new(0.033, 0.019, 0.021)),
                    0.012,
                );
                face.mesh(
                    &mut p.matte,
                    Vec3::new(-0.115, 0.012, 0.012),
                    Vec3::new(0.115, 0.228, 0.195),
                    0.0055,
                    &|at, _| {
                        let muzzle =
                            (1.0 - (at.x / 0.07).powi(2) - ((at.y - 0.069) / 0.045).powi(2))
                                .clamp(0.0, 1.0);
                        l.skin * (1.0 - muzzle * 0.12)
                    },
                );
                let mouth_height = |x: f32| 0.08 + (x / 0.046).powi(2) * 0.006;
                let mouth: [Vec2; 25] = std::array::from_fn(|i| {
                    let x = (i as f32 / 24.0 * 2.0 - 1.0) * 0.046;
                    Vec2::new(x, mouth_height(x))
                });
                surface_stroke(
                    &mut p.matte,
                    &mouth,
                    0.0032,
                    &|at| torso_surface_frame(&face, at, true),
                    l.skin * 0.42,
                );
                for side in [-1.0f32, 1.0] {
                    let (at, normal) =
                        torso_surface_frame(&face, Vec2::new(side * 0.013, 0.114), true);
                    p.matte.blob(
                        at + normal * 0.0012,
                        Vec3::new(0.006, 0.0035, 0.002),
                        0,
                        0.0,
                        l.skin * 0.42,
                    );
                }
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.15), 0.035, 0.016);
                for side in [-1.0f32, 1.0] {
                    p.matte.prism(
                        Vec3::new(side * 0.1, 0.24, 0.0),
                        Vec3::new(side * 0.18, 0.34, -0.06),
                        0.035,
                        0.0,
                        6,
                        BONE,
                        0.0,
                    );
                    let x = side * 0.035;
                    let (root, normal) =
                        torso_surface_frame(&face, Vec2::new(x, mouth_height(x)), true);
                    let root = root - normal * 0.002;
                    p.matte.prism(
                        root,
                        root + Vec3::new(side * 0.005, -0.04, 0.01),
                        0.012,
                        0.0,
                        4,
                        BONE,
                        0.0,
                    );
                }
            }
            _ => {
                // A slightly large head reads well from above.
                let size = if fem { 1.1 } else { 1.14 };
                p.set_transform(
                    Affine3A::from_translation(Vec3::Y * 0.08)
                        * Affine3A::from_scale(Vec3::splat(size))
                        * Affine3A::from_translation(Vec3::Y * -0.08),
                );
                human_face(p, &l, !matches!(style, Style::Hobgoblin | Style::Troll));
                let hood_down = matches!(style, Style::Caster | Style::Scout) && l.v.pattern >= 2;
                let hooded = matches!(
                    style,
                    Style::Caster | Style::Scout | Style::Sentinel | Style::Wraith
                ) && !hood_down;
                if !hooded
                    && !matches!(
                        style,
                        Style::Hobgoblin | Style::Orc | Style::Troll | Style::Medusa
                    )
                {
                    hair(p, &l);
                }
                if (l.v.beard
                    && matches!(
                        style,
                        Style::Hero | Style::Caster | Style::Scout | Style::Common | Style::Zombie
                    ))
                    || (style == Style::Leprechaun && !fem)
                {
                    beard(p, hair_color(&l));
                }
                match style {
                    Style::Hero => {}
                    Style::Sentinel => {
                        tube(
                            &mut p.metal,
                            &[
                                Vec3::Y * 0.04,
                                Vec3::Y * 0.14,
                                Vec3::Y * 0.26,
                                Vec3::Y * 0.31,
                            ],
                            &[(0.128, 0.132), (0.136, 0.14), (0.13, 0.134), (0.11, 0.112)],
                            STEEL,
                        );
                        p.metal
                            .blob(Vec3::Y * 0.3, Vec3::new(0.11, 0.05, 0.112), 0, 0.0, STEEL);
                        // The visor: a dark slit, a raised ridge down the face, breathing holes.
                        p.matte.cuboid(
                            Vec3::new(0.0, 0.17, 0.128),
                            Vec3::new(0.095, 0.011, 0.02),
                            Quat::IDENTITY,
                            DARK,
                        );
                        p.metal.cuboid(
                            Vec3::new(0.0, 0.12, 0.138),
                            Vec3::new(0.01, 0.06, 0.012),
                            Quat::IDENTITY,
                            STEEL * 1.1,
                        );
                        for side in [-1.0f32, 1.0] {
                            for k in 0..3 {
                                p.matte.blob(
                                    Vec3::new(side * 0.05, 0.085 + k as f32 * 0.02, 0.124),
                                    Vec3::splat(0.006),
                                    0,
                                    0.0,
                                    DARK,
                                );
                            }
                        }
                        p.metal.torus(
                            Vec3::Y * 0.05,
                            Vec3::Y,
                            0.13,
                            0.014,
                            18,
                            4,
                            Vec3::new(0.8, 0.62, 0.3),
                        );
                        if fem {
                            braid(p, Vec3::new(0.0, 0.05, -0.12), hair_color(&l));
                        }
                        // A plume of the house colour sweeping back from the crest.
                        for k in 0..6 {
                            let spread = (k as f32 - 2.5) * 0.012;
                            let base = Vec3::new(spread, 0.33, 0.03);
                            tube(
                                &mut p.matte,
                                &[
                                    base,
                                    base + Vec3::new(0.0, 0.09, -0.04),
                                    base + Vec3::new(spread, 0.1, -0.16),
                                    base + Vec3::new(spread * 2.0, 0.02, -0.26),
                                ],
                                &[
                                    (0.016, 0.022),
                                    (0.018, 0.026),
                                    (0.013, 0.018),
                                    (0.002, 0.002),
                                ],
                                l.cloth * (0.9 + 0.04 * k as f32),
                            );
                        }
                    }
                    Style::Caster | Style::Scout if hood_down => {
                        let hood = if style == Style::Caster {
                            l.cloth
                        } else {
                            Vec3::new(0.3, 0.42, 0.22)
                        };
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut cowl = Sculpt::default();
                            cowl.add(
                                Ellipsoid(Vec3::new(0.0, 0.0, -0.12), Vec3::new(0.12, 0.07, 0.06)),
                                0.0,
                            );
                            for side in [-1.0f32, 1.0] {
                                cowl.add(
                                    Ellipsoid(
                                        Vec3::new(side * 0.1, -0.02, -0.06),
                                        Vec3::new(0.05, 0.05, 0.07),
                                    ),
                                    0.05,
                                );
                            }
                            // The hollow of the hood, open toward the neck.
                            cowl.carve(
                                Ellipsoid(
                                    Vec3::new(0.0, 0.03, -0.1),
                                    Vec3::new(0.085, 0.045, 0.04),
                                ),
                                0.02,
                            );
                            cowl.mesh(
                                &mut p.matte,
                                Vec3::new(-0.2, -0.1, -0.22),
                                Vec3::new(0.2, 0.1, 0.02),
                                0.007,
                                &|at, _| hood * 0.95 * crate::sdf::Sculpt::weave(at),
                            );
                        }
                    }
                    Style::Caster | Style::Scout | Style::Wraith => {
                        let hood = match style {
                            Style::Caster => l.cloth,
                            Style::Wraith => l.cloth * 0.85,
                            _ => Vec3::new(0.3, 0.42, 0.22),
                        };
                        // A deep hood: the shell rounds over the head, a thick
                        // rolled rim frames the face, and the cloth drapes down
                        // the back of the neck onto the shoulders.
                        // One sculpted piece of cloth: a shell rounding over the
                        // head, the face opening cut cleanly with only a soft
                        // thickness at its edge, the cloth falling behind the
                        // neck onto the shoulders.
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut h = Sculpt::default();
                            h.add(
                                Ellipsoid(
                                    Vec3::new(0.0, 0.195, -0.035),
                                    Vec3::new(0.138, 0.165, 0.145),
                                ),
                                0.0,
                            )
                            .add(
                                Ellipsoid(Vec3::new(0.0, 0.07, -0.11), Vec3::new(0.12, 0.13, 0.07)),
                                0.05,
                            )
                            .add(
                                Ellipsoid(
                                    Vec3::new(0.0, -0.03, -0.09),
                                    Vec3::new(0.16, 0.05, 0.09),
                                ),
                                0.05,
                            )
                            // The face opening, tilted to follow the face.
                            .carve(
                                Ellipsoid(Vec3::new(0.0, 0.14, 0.13), Vec3::new(0.088, 0.13, 0.11)),
                                0.012,
                            )
                            // The underside around the neck and the front of the shoulders.
                            .carve(
                                Ellipsoid(Vec3::new(0.0, -0.06, 0.1), Vec3::new(0.2, 0.08, 0.14)),
                                0.02,
                            );
                            h.mesh(
                                &mut p.matte,
                                Vec3::new(-0.2, -0.1, -0.22),
                                Vec3::new(0.2, 0.38, 0.14),
                                0.0065,
                                &|_at, n| {
                                    // A slightly darker edge where the cloth turns inward.
                                    (if n.z > 0.6 { hood * 0.88 } else { hood })
                                        * crate::sdf::Sculpt::weave(_at)
                                },
                            );
                        }
                        if fem || style == Style::Wraith {
                            // Locks of hair escape the hood to frame the face.
                            let c = hair_color(&l);
                            for side in [-1.0f32, 1.0] {
                                lock(
                                    &mut p.matte,
                                    &[
                                        Vec3::new(side * 0.075, 0.2, 0.07),
                                        Vec3::new(side * 0.088, 0.13, 0.07),
                                        Vec3::new(side * 0.08, 0.05, 0.06),
                                    ],
                                    0.026,
                                    0.016,
                                    c,
                                );
                            }
                        }
                        // Some hoods end in a long soft point that flops back.
                        let pointed = style == Style::Scout || l.aura == Some(Accent::Ember);
                        if pointed {
                            tube(
                                &mut p.matte,
                                &[
                                    Vec3::new(0.0, 0.3, -0.06),
                                    Vec3::new(0.0, 0.37, -0.14),
                                    Vec3::new(0.0, 0.33, -0.24),
                                    Vec3::new(0.0, 0.22, -0.27),
                                ],
                                &[(0.075, 0.06), (0.05, 0.045), (0.03, 0.028), (0.004, 0.004)],
                                hood * 0.95,
                            );
                        }
                    }
                    Style::Hobgoblin => {
                        for side in [-1.0f32, 1.0] {
                            p.matte.prism(
                                Vec3::new(side * 0.1, 0.17, 0.0),
                                Vec3::new(side * 0.26, 0.23, -0.05),
                                0.035,
                                0.0,
                                5,
                                l.skin,
                                0.0,
                            );
                            p.matte.prism(
                                Vec3::new(side * 0.03, 0.08, 0.1),
                                Vec3::new(side * 0.035, 0.12, 0.11),
                                0.01,
                                0.0,
                                3,
                                BONE,
                                0.0,
                            );
                        }
                        // A big ruddy nose and a stiff dark mohawk, or, for her, a wild topknot.
                        p.matte.blob(
                            Vec3::new(0.0, 0.132, 0.115),
                            Vec3::new(0.04, 0.031, 0.045),
                            2,
                            0.0,
                            Vec3::new(0.78, 0.36, 0.3),
                        );
                        if fem {
                            let c = hair_color(&l);
                            p.matte.blob(
                                Vec3::new(0.0, 0.28, -0.04),
                                Vec3::new(0.035, 0.03, 0.035),
                                1,
                                0.0,
                                c,
                            );
                            for k in 0..5 {
                                let a = k as f32 * 1.25;
                                lock(
                                    &mut p.matte,
                                    &[
                                        Vec3::new(0.0, 0.29, -0.04),
                                        Vec3::new(a.cos() * 0.05, 0.33, -0.05 + a.sin() * 0.05),
                                        Vec3::new(a.cos() * 0.1, 0.27, -0.06 + a.sin() * 0.1),
                                    ],
                                    0.024,
                                    0.014,
                                    c,
                                );
                            }
                        }
                        for k in 0..(if fem { 0 } else { 6 }) {
                            let z = 0.06 - k as f32 * 0.045;
                            let root = Vec3::new(0.0, 0.25 - (k as f32 - 2.0).powi(2) * 0.004, z);
                            tube(
                                &mut p.matte,
                                &[root, root + Vec3::new(0.0, 0.06, -0.03)],
                                &[(0.012, 0.03), (0.002, 0.004)],
                                DARK * 3.0,
                            );
                        }
                    }
                    Style::Zombie => {
                        p.matte.blob(
                            Vec3::new(-0.06, 0.2, 0.08),
                            Vec3::new(0.025, 0.02, 0.015),
                            2,
                            0.3,
                            Vec3::new(0.35, 0.12, 0.1),
                        );
                    }
                    Style::Vampire => {
                        p.matte.prism(
                            Vec3::new(0.0, 0.25, 0.07),
                            Vec3::new(0.0, 0.18, 0.11),
                            0.03,
                            0.0,
                            4,
                            DARK,
                            0.0,
                        );
                    }
                    _ => rogue::head_details(p, &l),
                }
            }
        }
    });
    for (side, arm, fore, hand) in [
        (1.0f32, J::ArmL, J::ForeL, J::HandL),
        (-1.0, J::ArmR, J::ForeR, J::HandR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let shoulder = body.shoulder(side);
        let metal = style == Style::Sentinel;
        let upper = kit.joint(arm, torso, shoulder, &format!("arm{tag}"), |p| {
            let m = if metal { &mut p.metal } else { &mut p.matte };
            if blocky {
                stone(m, Vec3::Y * -0.16, Vec3::new(0.085, 0.17, 0.085), sleeve, 8);
                return;
            }
            if furry {
                m.blob(
                    Vec3::new(-side * 0.012, -0.02, 0.0),
                    Vec3::new(0.062, 0.06, 0.062) * b,
                    2,
                    0.0,
                    sleeve,
                );
                tube(
                    m,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.1 * arm_len, 0.006),
                        Vec3::Y * -0.2 * arm_len,
                        Vec3::Y * -0.3 * arm_len,
                    ],
                    &[
                        (0.06 * b, 0.06 * b),
                        (0.064 * b, 0.068 * b),
                        (0.058 * b, 0.06 * b),
                        (0.05 * b, 0.05 * b),
                    ],
                    sleeve,
                );
            } else if metal || bare_arms {
                biped_upper_arm(m, side, b, arm_len, body.profile.muscle, true, sleeve);
            } else {
                biped_upper_sleeve(m, side, b, arm_len, body.profile.muscle, sleeve);
            }
            if furry {
                m.blob(Vec3::Y * -0.12, Vec3::new(0.1, 0.16, 0.1), 3, 0.1, sleeve);
                // Shaggy fur spilling off the shoulder.
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.02, 0.0),
                    0.1,
                    8,
                    Vec3::new(side, -1.2, 0.0),
                    l.cloth * 0.95,
                    29 + side as u64,
                );
            }
            if metal {
                biped_pauldron(p, side, b);
                // Rerebrace lames below the pauldron, each tucked under the last.
                plate_lames(
                    &mut p.metal,
                    -0.08 * arm_len,
                    3,
                    0.055 * arm_len,
                    0.064 * b,
                    0.058 * b,
                );
            }
        });
        let lower = kit.joint(fore, upper, body.elbow(), &format!("fore{tag}"), |p| {
            if blocky {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.15,
                    Vec3::new(0.075, 0.15, 0.075),
                    l.skin * 0.9,
                    9,
                );
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.34,
                    Vec3::new(0.105, 0.085, 0.105),
                    l.skin,
                    10,
                );
                return;
            }
            if metal {
                // An elbow cop with a fan.
                p.metal.blob(
                    Vec3::new(0.0, 0.0, -0.01),
                    Vec3::new(0.07, 0.065, 0.07),
                    2,
                    0.0,
                    STEEL * 1.05,
                );
                p.metal.blob(
                    Vec3::new(side * 0.055, 0.0, -0.01),
                    Vec3::new(0.012, 0.05, 0.045),
                    2,
                    0.0,
                    STEEL,
                );
            }
            let m = if metal { &mut p.metal } else { &mut p.matte };
            if metal {
                m.blob(Vec3::ZERO, Vec3::splat(0.058 * b), 2, 0.0, sleeve);
                tube(
                    m,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.07 * arm_len, 0.004),
                        Vec3::Y * -0.17 * arm_len,
                        Vec3::Y * -0.26 * arm_len,
                    ],
                    &[
                        (0.05 * b, 0.05 * b),
                        (
                            0.06 * b * body.profile.muscle,
                            0.056 * b * body.profile.muscle,
                        ),
                        (
                            0.05 * b * (0.5 + 0.5 * body.profile.muscle),
                            0.046 * b * (0.5 + 0.5 * body.profile.muscle),
                        ),
                        (0.04 * b, 0.036 * b),
                    ],
                    sleeve,
                );
            } else {
                biped_forearm(m, b, arm_len, body.profile.muscle, bare_arms, sleeve);
            }
            let cuff = match style {
                Style::Hero | Style::Scout | Style::Hobgoblin => Some(LEATHER),
                Style::Orc | Style::Troll | Style::Nymph | Style::Phantom => Some(LEATHER * 0.85),
                Style::Leprechaun => Some(CREAM),
                Style::Medusa => Some(l.trim),
                Style::Sentinel => Some(STEEL * 0.85),
                Style::Caster | Style::Vampire | Style::Wraith => Some(l.trim),
                _ => None,
            };
            if style == Style::Caster {
                biped_cuff(m, b, arm_len, body.profile.muscle, true, sleeve, l.trim);
            } else if let Some(cuff) = cuff {
                if metal {
                    // Keep the vambrace separate from its elbow cop.
                    m.prism(
                        Vec3::Y * -0.13 * arm_len,
                        Vec3::Y * -0.25 * arm_len,
                        0.056 * b,
                        0.05 * b,
                        8,
                        cuff,
                        0.0,
                    );
                } else {
                    biped_cuff(m, b, arm_len, body.profile.muscle, false, cuff, cuff * 0.86);
                }
            }
            if furry {
                p.matte.blob(
                    Vec3::Y * -0.1,
                    Vec3::new(0.085, 0.15, 0.085),
                    4,
                    0.1,
                    l.cloth,
                );
                hair_tufts(
                    p,
                    Vec3::Y * -0.06,
                    0.085,
                    7,
                    Vec3::new(0.0, -1.0, 0.0),
                    l.cloth * 0.97,
                    43 + side as u64,
                );
            }
        });
        let grip = kit
            .commands
            .spawn((
                Transform::from_translation(body.hand).with_scale(Vec3::splat(1.22)),
                Visibility::default(),
            ))
            .id();
        kit.commands.entity(lower).add_child(grip);
        kit.rig.joints[hand as usize] = Some(grip);
        kit.rig.rest[hand as usize] = body.hand;
        if !blocky {
            kit.attach(grip, Transform::IDENTITY, &format!("hand{tag}"), |p| {
                // Equipment presentation attaches props to HandR. The free
                // left hand can express the role without opening a held grip.
                let clawed = matches!(
                    style,
                    Style::Zombie
                        | Style::Vampire
                        | Style::Troll
                        | Style::Medusa
                        | Style::Phantom
                        | Style::Wraith
                );
                let open =
                    clawed || matches!(style, Style::Caster | Style::Leprechaun | Style::Nymph);
                if side > 0.0 && open {
                    open_hand(p, Vec3::Y * (0.01 * arm_len), side, b, hand_color, clawed);
                } else {
                    fist(p, Vec3::Y * (0.01 * arm_len), side, b, hand_color);
                }
                if furry {
                    for k in 0..3 {
                        let x = (k as f32 - 1.0) * 0.03;
                        p.matte.prism(
                            Vec3::new(x, -0.09 * arm_len, 0.04),
                            Vec3::new(x, -0.14 * arm_len, 0.09),
                            0.012,
                            0.0,
                            4,
                            BONE,
                            0.0,
                        );
                    }
                }
            });
        }
    }
    for (side, leg, shin, foot) in [
        (1.0f32, J::LegL, J::ShinL, J::FootL),
        (-1.0, J::LegR, J::ShinR, J::FootR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, hips, body.leg_root(side), &format!("leg{tag}"), |p| {
            if blocky {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.22,
                    Vec3::new(0.095, 0.22, 0.095),
                    pants,
                    11,
                );
                return;
            }
            if furry {
                p.matte
                    .blob(Vec3::ZERO, Vec3::splat(0.094 * b), 3, 0.0, pants);
                tube(
                    &mut p.matte,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.13, 0.008),
                        Vec3::Y * -0.3,
                        Vec3::Y * -0.44,
                    ],
                    &[
                        (0.094 * b, 0.096 * b),
                        (0.09 * b, 0.096 * b),
                        (0.072 * b, 0.074 * b),
                        (0.06 * b, 0.062 * b),
                    ],
                    pants,
                );
            } else if style == Style::Sentinel {
                // Cuisses over the mail: overlapping lames down to the knee cop.
                sculpted_limb(&mut p.matte, body.thigh, pants);
                plate_lames(&mut p.metal, -0.04, 4, 0.085, 0.104 * b, 0.075 * b);
            } else if trousered {
                // Trousers ease over the thigh, which stays fullest above the middle.
                trouser_leg(&mut p.matte, body.thigh, b, false, pants);
            } else {
                // Hip to knee: the thigh is fullest above the middle.
                sculpted_limb(&mut p.matte, body.thigh, pants);
            }
            if furry {
                p.matte
                    .blob(Vec3::Y * -0.18, Vec3::new(0.12, 0.2, 0.12), 5, 0.1, pants);
                hair_tufts(
                    p,
                    Vec3::Y * -0.12,
                    0.11,
                    8,
                    Vec3::new(0.0, -1.0, 0.0),
                    pants * 0.97,
                    47 + side as u64,
                );
            }
        });
        let lower_leg = kit.joint(shin, thigh, body.knee(), &format!("shin{tag}"), |p| {
            if blocky {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.2,
                    Vec3::new(0.088, 0.2, 0.088),
                    pants * 0.92,
                    12,
                );
                return;
            }
            if style == Style::Sentinel {
                // A knee cop with a wing.
                p.metal.blob(
                    Vec3::new(0.0, 0.0, 0.03),
                    Vec3::new(0.07, 0.07, 0.06),
                    2,
                    0.0,
                    STEEL * 1.05,
                );
                p.metal.blob(
                    Vec3::new(side * 0.05, 0.0, 0.02),
                    Vec3::new(0.015, 0.05, 0.04),
                    2,
                    0.0,
                    STEEL,
                );
            }
            // Knee, calf, boot with a turned-down cuff, foot and toe.
            if furry {
                p.matte
                    .blob(Vec3::ZERO, Vec3::splat(0.07 * b), 4, 0.0, pants);
                tube(
                    &mut p.matte,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.13, -0.012),
                        Vec3::Y * -0.28,
                        Vec3::Y * -0.4,
                    ],
                    &[
                        (0.062 * b, 0.064 * b),
                        (0.07 * b, 0.076 * b),
                        (0.054 * b, 0.056 * b),
                        (0.05 * b, 0.05 * b),
                    ],
                    pants,
                );
            } else if trousered {
                // Cloth gathers below the knee before the boot.
                trouser_leg(&mut p.matte, body.shin, b, true, pants);
            } else {
                // Knee to ankle, the calf full at the back.
                sculpted_limb(&mut p.matte, body.shin, pants);
            }
            let boot_top = boot_shape.top;
            p.satin.prism(
                Vec3::Y * boot_top,
                Vec3::Y * boot_shape.bottom,
                boot_shape.radius_top,
                boot_shape.radius_bottom,
                8,
                boot,
                0.0,
            );
            if barefoot {
                // Wrappings bind the ankle and calf of a bare leg.
                for y in [-0.3f32, -0.36] {
                    p.satin
                        .torus(Vec3::Y * y, Vec3::Y, 0.062 * b, 0.01, 12, 3, LEATHER * 0.8);
                }
            } else if !matches!(style, Style::Zombie | Style::Yeti) {
                let (radius, thickness) = (boot_shape.cuff_radius, boot_shape.cuff_thickness);
                p.satin.torus(
                    Vec3::Y * boot_top,
                    Vec3::Y,
                    radius,
                    thickness,
                    12,
                    3,
                    boot * 1.15,
                );
            }
            if matches!(
                style,
                Style::Hero | Style::Scout | Style::Hobgoblin | Style::Orc | Style::Leprechaun
            ) {
                for y in [-0.3f32, -0.37] {
                    p.satin
                        .torus(Vec3::Y * y, Vec3::Y, 0.068 * b, 0.009, 12, 3, boot * 0.6);
                    p.metal.cuboid(
                        Vec3::new(side * 0.06 * b, y, 0.02),
                        Vec3::new(0.008, 0.014, 0.014),
                        Quat::IDENTITY,
                        Vec3::new(0.74, 0.5, 0.26),
                    );
                }
            }
        });
        kit.joint(foot, lower_leg, body.ankle(), &format!("foot{tag}"), |p| {
            p.set_transform(Affine3A::from_translation(Vec3::Y * 0.4));
            if blocky {
                // The same stone foot, now articulated at the ankle
                // so its broad sole can stay level during support.
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, -0.43, 0.05),
                    Vec3::new(0.105, 0.045, 0.16),
                    boot,
                    13,
                );
                return;
            }
            // A rounded boot: heel, instep and a broad toe, on a darker sole.
            for (i, (center, radii)) in body.feet.into_iter().enumerate() {
                p.satin.blob(
                    center - Vec3::Y * 0.4,
                    radii,
                    (2 - i) as u64,
                    0.0,
                    if i == 2 { boot * 0.55 } else { boot },
                );
            }
            if furry || style == Style::Troll {
                for k in 0..3 {
                    let x = (k as f32 - 1.0) * 0.035;
                    p.matte.prism(
                        Vec3::new(x, -0.44, 0.17),
                        Vec3::new(x, -0.46, 0.22),
                        0.012,
                        0.0,
                        4,
                        BONE,
                        0.0,
                    );
                }
            }
        });
    }
    rogue::biped_extras(kit, &l, body, torso, hips);
    // Capes and cloaks hang from the shoulders.
    if matches!(style, Style::Hero | Style::Vampire) {
        let (cloak, lining) = if style == Style::Vampire {
            (DARK, l.trim)
        } else {
            (l.cloth * 0.62, l.trim)
        };
        kit.drape(
            torso,
            Transform::from_xyz(0.0, 0.53, -0.15),
            Drape::Cloak,
            "cloak",
            |p| {
                // The cloak hangs in soft flutes from a gathered yoke across the
                // shoulders, fanning wider toward the hem; the lining shows
                // between the folds at the edges.
                let folds = 6;
                for k in 0..folds {
                    let t = k as f32 / (folds - 1) as f32 * 2.0 - 1.0;
                    let top = Vec3::new(
                        t * 0.17 * body.profile.shoulders,
                        0.0,
                        -0.01 - 0.02 * (1.0 - t * t),
                    );
                    let bottom = Vec3::new(
                        t * 0.3 * body.profile.hips,
                        -0.98 + 0.03 * (k as f32 * 1.7).sin(),
                        -0.13 - 0.05 * (1.0 - t * t),
                    );
                    let mid = top.lerp(bottom, 0.5) + Vec3::new(0.0, 0.0, -0.03);
                    let c = if t.abs() > 0.9 {
                        lining
                    } else {
                        cloak * (0.94 + 0.06 * (k as f32 * 2.3).sin())
                    };
                    tube(
                        &mut p.matte,
                        &[top, mid, bottom],
                        &[
                            (0.045 * body.profile.shoulders, 0.025),
                            (
                                0.055 * (body.profile.shoulders + body.profile.hips) * 0.5,
                                0.03,
                            ),
                            (0.06 * body.profile.hips, 0.028),
                        ],
                        c,
                    );
                }
                p.matte.blob(
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(0.22 * body.profile.shoulders, 0.04, 0.06),
                    2,
                    0.0,
                    cloak,
                );
            },
        );
    }
}

/// Seat sockets on the actual skull surface and return a local frame for
/// each eye. Animals look out to the sides as well as forward; a forward-only
/// eye can disappear behind a lateral brow or show through the opposite cheek.
fn animal_eye_sockets(
    skull: &mut crate::sdf::Sculpt,
    center: Vec3,
    spread: f32,
    size: f32,
    outward: Vec3,
) -> [(Vec3, Vec3); 2] {
    use crate::sdf::Shape::Ellipsoid;
    [-1.0f32, 1.0].map(|side| {
        let normal = Vec3::new(side * outward.x, outward.y, outward.z).normalize();
        let hint = center + Vec3::X * side * spread;
        let at = skull.onto(hint + normal * 0.18, -normal);
        skull.carve(
            Ellipsoid(at + normal * size * 0.18, Vec3::splat(size * 1.02)),
            size * 0.16,
        );
        (at - normal * size * 0.12, normal)
    })
}

/// An ellipsoid expressed in an eye's tangent frame. The builder still owns
/// the part transform, so larger storybook skulls keep correctly oriented eyes.
fn animal_eye_lobe(b: &mut Builder, center: Vec3, normal: Vec3, radii: Vec3, color: Vec3) {
    let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
    let up = normal.cross(right).normalize();
    let point = |row: usize, column: usize| {
        let latitude = row as f32 * PI / 8.0;
        let longitude = column as f32 * TAU / 12.0;
        let local = Vec3::new(
            latitude.sin() * longitude.cos(),
            latitude.cos(),
            latitude.sin() * longitude.sin(),
        );
        let at = center
            + right * local.x * radii.x
            + up * local.y * radii.y
            + normal * local.z * radii.z;
        let n = (right * local.x / radii.x + up * local.y / radii.y + normal * local.z / radii.z)
            .normalize();
        (at, n)
    };
    for row in 0..8 {
        for column in 0..12 {
            let a = point(row, column);
            let b0 = point(row + 1, column);
            let c = point(row + 1, column + 1);
            let d = point(row, column + 1);
            for (enabled, mut face) in [(row > 0, [a, b0, d]), (row < 7, [b0, c, d])] {
                if enabled {
                    if (face[1].0 - face[0].0)
                        .cross(face[2].0 - face[0].0)
                        .dot(face[0].1 + face[1].1 + face[2].1)
                        < 0.0
                    {
                        face.swap(1, 2);
                    }
                    b.tri_smooth(face.map(|v| v.0), face.map(|v| v.1), [color; 3]);
                }
            }
        }
    }
}

/// Bold, hooded animal eyes inside broad carved sockets. Their coloured iris
/// fills most of the opening, giving expression without exposed white spheres.
fn animal_eyes(
    p: &mut Parts,
    frames: [(Vec3, Vec3); 2],
    size: f32,
    iris: Vec3,
    skin: Vec3,
    accent: Option<Accent>,
) {
    if let Some(accent) = accent {
        p.accent = Some(accent);
    }
    for (at, normal) in frames {
        let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
        let up = normal.cross(right).normalize();
        animal_eye_lobe(
            &mut p.matte,
            at,
            normal,
            Vec3::new(size * 1.08, size * 0.86, size * 0.65),
            DARK * 1.4,
        );
        let colored = if accent.is_some() {
            &mut p.glow
        } else {
            &mut p.satin
        };
        animal_eye_lobe(
            colored,
            at + normal * size * 0.54,
            normal,
            Vec3::new(size * 0.8, size * 0.73, size * 0.26),
            if accent.is_some() { Vec3::ONE } else { iris },
        );
        if accent.is_none() {
            animal_eye_lobe(
                &mut p.satin,
                at + normal * size * 0.78,
                normal,
                Vec3::new(size * 0.32, size * 0.54, size * 0.12),
                DARK,
            );
            animal_eye_lobe(
                &mut p.satin,
                at + normal * size * 0.87 + (right + up) * size * 0.23,
                normal,
                Vec3::splat(size * 0.13),
                Vec3::splat(1.5),
            );
        }
        tube(
            &mut p.matte,
            &[
                at - right * size * 1.1 + up * size * 0.12,
                at + up * size * 0.8 + normal * size * 0.36,
                at + right * size * 1.1 + up * size * 0.12,
            ],
            &[
                (size * 0.2, size * 0.18),
                (size * 0.26, size * 0.2),
                (size * 0.2, size * 0.18),
            ],
            skin * 0.95,
        );
    }
}

/// The emu: a shaggy coat of drooping feathers on long scaly legs, an
/// S-curved blue neck and a quick beak.
/// Splitting a terminal mesh off its old parent must preserve the parent's
/// baked Storybook proportions. In neutral pose, pivot + terminal_scale *
/// this_transform * vertex is exactly the old parent_scale * vertex.
fn terminal_part_transform(plan: Plan, parent: J, terminal: J, pivot: Vec3) -> Affine3A {
    let parent_scale = storybook_part(plan, parent);
    let terminal_scale = storybook_part(plan, terminal);
    Affine3A::from_translation(-pivot / terminal_scale)
        * Affine3A::from_scale(parent_scale / terminal_scale)
}

/// Bone endpoint shared by the bird mesh and its support solver.
pub(super) const BIRD_ANKLE: Vec3 = Vec3::new(0.0, -0.46, -0.06);

fn bird(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let body = kit.joint(J::Body, lift, Vec3::new(0.0, 1.0, 0.0), "body", |p| {
        p.matte.blob(
            Vec3::ZERO,
            Vec3::new(0.245, 0.26, 0.355),
            11,
            0.0,
            l.skin * 0.7,
        );
        // Each shingle travels down the ovoid, turning inward below its
        // widest latitude. Lower rows shorten toward the belly rather than
        // all flaring out into one horizontal skirt.
        for row in 0..7 {
            let latitude = 0.12 + row as f32 * 0.31;
            let count = 14 + row * 4;
            for k in 0..count {
                let n = (row * 50 + k) as u32;
                let longitude =
                    (k as f32 + 0.5 * (row % 2) as f32 + rand(84, n) * 0.3) / count as f32 * TAU;
                let sweep = 0.5 + 0.08 * rand(81, n);
                let path: [Vec3; 4] = std::array::from_fn(|i| {
                    let t = i as f32 / 3.0;
                    let a = latitude + sweep * t;
                    let normal = Vec3::new(
                        a.sin() * longitude.cos(),
                        a.cos(),
                        a.sin() * longitude.sin(),
                    );
                    let surface = normal * Vec3::new(0.245, 0.26, 0.355);
                    // Fine tips leave the surface only slightly. This retains
                    // shaggy texture around a connected body silhouette.
                    surface
                        + normal * (0.012 + 0.012 * (PI * t).sin())
                        + Vec3::NEG_Y * (0.024 * t * t)
                });
                let width = (0.052 + 0.004 * row as f32) * 0.5;
                let radii = [
                    Vec2::new(width * 0.4, 0.006),
                    Vec2::new(width, 0.009),
                    Vec2::new(width * 0.62, 0.006),
                    Vec2::splat(0.0015),
                ];
                let across = Vec3::new(-longitude.sin(), 0.0, longitude.cos());
                let shade = 0.82 + 0.05 * row as f32 / 6.0 + 0.26 * rand(82, n);
                p.matte.tube_framed(
                    &path,
                    &radii,
                    6,
                    &|_, _, t| l.skin * shade * (1.0 - 0.13 * t),
                    Some(across),
                );
            }
        }
        for k in 0..7 {
            let a = (k as f32 - 3.0) * 0.28;
            feather(
                p,
                Vec3::new(a * 0.08, 0.08, -0.3),
                Vec3::new(a * 0.4, 0.0, -1.0).normalize(),
                0.24,
                0.048,
                l.skin * (0.85 + 0.05 * k as f32),
            );
        }
    });
    let neck = kit.joint(J::Neck, body, Vec3::new(0.0, 0.12, 0.3), "neck", |p| {
        // One smooth S of a neck, thick at the base.
        tube(
            &mut p.matte,
            &[
                Vec3::ZERO,
                Vec3::new(0.0, 0.16, 0.08),
                Vec3::new(0.0, 0.32, 0.03),
                Vec3::new(0.0, 0.47, 0.08),
                Vec3::new(0.0, 0.5, 0.1),
            ],
            &[
                (0.085, 0.09),
                (0.058, 0.062),
                (0.047, 0.05),
                (0.042, 0.044),
                (0.04, 0.04),
            ],
            l.cloth,
        );
        // A ruff of feathers where the neck meets the body.
        for k in 0..12 {
            let a = k as f32 * TAU / 12.0;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            feather(
                p,
                out * 0.06 + Vec3::Y * 0.06,
                out,
                0.11,
                0.045,
                l.skin * (0.9 + 0.2 * rand(83, k)),
            );
        }
    });
    kit.joint(J::Head, neck, Vec3::new(0.0, 0.47, 0.08), "head", |p| {
        use crate::sdf::{Sculpt, Shape::*};
        let mut skull = Sculpt::default();
        skull
            .add(
                Ellipsoid(Vec3::new(0.0, 0.03, 0.02), Vec3::new(0.06, 0.058, 0.075)),
                0.0,
            )
            .add(
                Ellipsoid(Vec3::new(0.0, 0.005, -0.007), Vec3::new(0.047, 0.05, 0.055)),
                0.02,
            );
        let eye_frames = animal_eye_sockets(
            &mut skull,
            Vec3::new(0.0, 0.047, 0.043),
            0.048,
            0.014,
            Vec3::new(0.88, 0.08, 0.48),
        );
        skull.mesh(
            &mut p.matte,
            Vec3::new(-0.085, -0.06, -0.08),
            Vec3::new(0.085, 0.11, 0.115),
            0.0045,
            &|_, _| l.cloth,
        );
        p.matte.prism(
            Vec3::new(0.0, 0.02, 0.07),
            Vec3::new(0.0, 0.0, 0.17),
            0.028,
            0.006,
            6,
            DARK * 2.5,
            0.0,
        );
        p.matte.prism(
            Vec3::new(0.0, 0.005, 0.07),
            Vec3::new(0.0, -0.005, 0.15),
            0.018,
            0.004,
            5,
            DARK * 2.0,
            0.0,
        );
        animal_eyes(
            p,
            eye_frames,
            0.014,
            Vec3::new(0.72, 0.4, 0.08),
            l.cloth,
            None,
        );
        // A shaggy dark crest.
        hair_tufts(
            p,
            Vec3::new(0.0, 0.08, -0.01),
            0.04,
            7,
            Vec3::new(0.0, 0.6, -1.0),
            l.skin * 0.55,
            19,
        );
    });
    for (side, leg, shin, foot) in [
        (1.0f32, J::LegL, J::ShinL, J::FootL),
        (-1.0, J::LegR, J::ShinR, J::FootR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(
            leg,
            body,
            Vec3::new(side * 0.11, -0.1, 0.0),
            &format!("leg{tag}"),
            |p| {
                p.matte.blob(
                    Vec3::new(0.0, -0.08, 0.02),
                    Vec3::new(0.08, 0.15, 0.09),
                    3,
                    0.15,
                    l.skin * 0.85,
                );
                p.matte.prism(
                    Vec3::new(0.0, -0.15, 0.03),
                    Vec3::new(0.0, -0.4, 0.08),
                    0.045,
                    0.032,
                    6,
                    l.trim,
                    0.0,
                );
            },
        );
        let lower = kit.joint(
            shin,
            thigh,
            Vec3::new(0.0, -0.4, 0.08),
            &format!("shin{tag}"),
            |p| {
                p.matte.blob(Vec3::ZERO, Vec3::splat(0.036), 2, 0.0, l.trim);
                p.matte.prism(
                    Vec3::ZERO,
                    Vec3::new(0.0, -0.46, -0.06),
                    0.03,
                    0.022,
                    6,
                    l.trim * 0.95,
                    0.0,
                );
                for k in 0..6 {
                    let y = -0.08 - k as f32 * 0.06;
                    p.matte.torus(
                        Vec3::new(0.0, y, -0.01 * k as f32),
                        Vec3::Y,
                        0.026,
                        0.0028,
                        8,
                        3,
                        l.trim * 0.8,
                    );
                }
            },
        );
        let foot_transform = terminal_part_transform(Plan::Bird, shin, foot, BIRD_ANKLE);
        kit.joint(foot, lower, BIRD_ANKLE, &format!("foot{tag}"), |p| {
            // Keep the existing toes and ankle cap in exactly their neutral
            // positions, but let the sole turn independently of the shin.
            p.set_transform(foot_transform);
            p.matte.blob(
                Vec3::new(0.0, -0.46, -0.06),
                Vec3::splat(0.03),
                2,
                0.0,
                l.trim,
            );
            for k in -1..=1 {
                let a = k as f32 * 0.45;
                let toe = Vec3::new(a.sin() * 0.11, -0.49, -0.06 + a.cos() * 0.13);
                p.matte.prism(
                    Vec3::new(0.0, -0.47, -0.06),
                    toe,
                    0.018,
                    0.01,
                    5,
                    l.trim * 0.8,
                    0.0,
                );
                p.matte.prism(
                    toe,
                    toe + Vec3::new(a.sin(), -0.2, a.cos()) * 0.03,
                    0.01,
                    0.0,
                    4,
                    DARK * 3.0,
                    0.0,
                );
            }
        });
    }
}

/// Blue-violet charcoal keeps a black coat readable under ordinary light.
/// The dorsal planes carry a little more pigment value; this is not emission.
fn equine_hide(base: Vec3, style: Style, normal: Vec3) -> Vec3 {
    if style == Style::Unicorn {
        let dorsal = (normal.y * 0.5 + 0.5).clamp(0.0, 1.0);
        base.lerp(Vec3::new(0.23, 0.25, 0.34), dorsal * 0.3)
    } else {
        base
    }
}

/// Horses and their kin; with `centaur`, a rider's torso rises from the withers.
/// The pastern/hoof pivot, shared with analytical leg posing.
pub(super) const EQUINE_ANKLE: Vec3 = Vec3::new(0.0, -0.42, 0.02);

fn quadruped(kit: &mut Kit, lift: Entity, look: &Look, centaur: bool) {
    let l = look.clone();
    let style = l.style;
    let quagga = style == Style::Quagga;
    // Hip offset + upper leg + hoof centre + sole thickness. All four
    // unposed soles meet the floor; movement may crouch this rest height.
    let body_height = 0.1 + 0.42 + 0.482 + 0.006;
    // A quagga's coat: chestnut behind, cream and brown stripes over the
    // forequarters, neck and face, a pale belly. Painted, not banded.
    let coat = move |dir: Vec3, at: Vec3| -> Vec3 {
        if !quagga {
            return equine_hide(l.skin, style, dir);
        }
        let dark = Vec3::new(0.36, 0.2, 0.1);
        let cream = l.cloth;
        // A cream chest and belly; stripes band the sides.
        if dir.y < -0.5 || (dir.z > 0.45 && at.z > 0.2) {
            return cream;
        }
        let front = ease((at.z + 0.1) / 0.5);
        // Each quagga's stripes are its own: spacing, slant and phase.
        let pv = f32::from(l.v.pattern);
        let wave = (at.z * (30.0 + 3.0 * pv)
            + at.y * (4.0 + 2.5 * (pv - 1.5))
            + (at.y * 5.0).sin() * 1.2
            + pv * 1.7)
            .sin();
        let striped = cream.lerp(dark, ((wave - 0.15) * 6.0).clamp(0.0, 1.0));
        l.skin.lerp(striped, front)
    };
    let body = kit.joint(
        J::Body,
        lift,
        Vec3::new(0.0, body_height, 0.0),
        "body",
        |p| {
            // One continuous hide over the rib cage, scapulae and haunches. The
            // limbs emerge inside these masses; separate round shoulder patches
            // make every horse, unicorn and centaur look assembled from toys.
            use crate::sdf::{Sculpt, Shape::*};
            let mut barrel = Sculpt::default();
            barrel
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.015, -0.02), Vec3::new(0.195, 0.225, 0.49)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.065, 0.35), Vec3::new(0.15, 0.205, 0.19)),
                    0.08,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.025, -0.43), Vec3::new(0.195, 0.21, 0.2)),
                    0.075,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.16, 0.29), Vec3::new(0.085, 0.08, 0.19)),
                    0.065,
                );
            for side in [-1.0f32, 1.0] {
                barrel
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.06, -0.125, 0.395),
                            Vec3::new(0.09, 0.15, 0.125),
                        ),
                        0.045,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.1, 0.005, 0.34),
                            Vec3::new(0.108, 0.205, 0.165),
                        ),
                        0.08,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.105, 0.015, -0.4),
                            Vec3::new(0.13, 0.215, 0.2),
                        ),
                        0.08,
                    );
            }
            barrel.mesh(
                &mut p.matte,
                Vec3::new(-0.32, -0.35, -0.7),
                Vec3::new(0.32, 0.3, 0.66),
                0.012,
                &|at, normal| coat(normal, at),
            );
        },
    );
    let neck_at = Vec3::new(0.0, 0.12, 0.45);
    if centaur {
        let torso = kit.joint(J::Torso, body, Vec3::new(0.0, 0.15, 0.42), "torso", |p| {
            // One torso rising out of the withers: waist, ribs, chest.
            let fem = l.v.woman;
            let w = if fem { 0.88 } else { 1.0 };
            let body_shape = {
                // Sculpted, as the people's torsos are: the waist rising out
                // of the withers into ribs, chest and rounded shoulders.
                use crate::sdf::{Sculpt, Shape::*};
                let mut body = Sculpt::default();
                body.add(
                    // The lower human root sits inside the withers instead
                    // of hanging out over the horse's breast below the belt.
                    Ellipsoid(Vec3::new(0.0, -0.1, -0.055), Vec3::new(0.15, 0.15, 0.12)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.15 * w, 0.14, 0.12)),
                    0.06,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.3, 0.005), Vec3::new(0.2 * w, 0.17, 0.13)),
                    0.06,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.44, -0.01), Vec3::new(0.2 * w, 0.1, 0.11)),
                    0.14,
                )
                .add(
                    Capsule(
                        Vec3::new(0.0, 0.5, -0.005),
                        Vec3::new(0.0, 0.6, -0.005),
                        0.065,
                        0.055,
                    ),
                    0.04,
                );
                for side in [-1.0f32, 1.0] {
                    body.add(
                        Ellipsoid(
                            Vec3::new(side * 0.235 * w, 0.47, 0.0),
                            Vec3::new(0.072, 0.068, 0.075),
                        ),
                        0.06,
                    );
                }
                if fem {
                    for side in [-1.0f32, 1.0] {
                        body.add(
                            Ellipsoid(
                                Vec3::new(side * 0.075, 0.34, 0.09),
                                Vec3::new(0.09, 0.085, 0.085),
                            ),
                            0.045,
                        );
                    }
                } else {
                    // The male rider has the same broad continuous chest
                    // plane as the bipeds, with no paired forward cups.
                    body.add(
                        Ellipsoid(Vec3::new(0.0, 0.38, 0.015), Vec3::new(0.185, 0.1, 0.1)),
                        0.04,
                    )
                    .add(
                        Ellipsoid(Vec3::new(0.0, 0.17, 0.075), Vec3::new(0.07, 0.09, 0.04)),
                        0.04,
                    );
                }
                body.mesh(
                    &mut p.matte,
                    Vec3::new(-0.34, -0.3, -0.22),
                    Vec3::new(0.34, 0.64, 0.27),
                    0.0085,
                    &|at, _| {
                        // The wrapped leather follows the sculpted chest and
                        // waist, preserving the figure instead of a rigid tube.
                        let neckline = 0.405 - 0.035 * (1.0 - (at.x / 0.17).abs().min(1.0));
                        if at.y < 0.035 {
                            // The coat transition is concealed by the belt;
                            // any overlap below it belongs to the equine body.
                            l.skin
                        } else if fem && at.y > 0.215 && at.y < neckline {
                            LEATHER
                        } else {
                            l.cloth
                        }
                    },
                );
                body
            };
            // A close-fitting diagonal baldric follows the chest and back;
            // it never projects as a rectangular plank past the shoulders.
            for front in [false, true] {
                fitted_ribbon(
                    &mut p.satin,
                    &body_shape,
                    &[
                        (Vec2::new(-0.105, 0.1), 0.026),
                        (Vec2::new(-0.055, 0.24), 0.026),
                        (Vec2::new(0.04, 0.37), 0.026),
                        (Vec2::new(0.15, 0.48), 0.025),
                    ],
                    front,
                    0.01,
                    LEATHER * 0.72,
                );
            }
            let (buckle, normal) = torso_surface_frame(&body_shape, Vec2::new(0.04, 0.37), true);
            p.metal.blob(
                buckle + normal * 0.018,
                Vec3::splat(0.018),
                0,
                0.0,
                Vec3::new(0.74, 0.5, 0.26),
            );
            tube(
                &mut p.satin,
                &[Vec3::Y * 0.015, Vec3::Y * 0.035, Vec3::Y * 0.055],
                &[(0.164, 0.15), (0.16, 0.146), (0.157, 0.143)],
                LEATHER * 0.72,
            );
        });
        kit.joint(J::Head, torso, Vec3::Y * 0.6, "head", |p| {
            // The rider's face and hair are a person's; men wear full beards.
            let rider = Look {
                skin: l.cloth,
                ..l.clone()
            };
            biped_neck(
                &mut p.matte,
                1.0,
                if rider.v.woman { 0.85 } else { 1.0 },
                l.cloth,
            );
            p.set_transform(
                Affine3A::from_translation(Vec3::Y * 0.08)
                    * Affine3A::from_scale(Vec3::splat(1.1))
                    * Affine3A::from_translation(Vec3::Y * -0.08),
            );
            human_face(p, &rider, true);
            hair(p, &rider);
            if !rider.v.woman {
                beard(p, hair_color(&rider));
            }
        });
        for (side, arm, fore, hand) in [
            (1.0f32, J::ArmL, J::ForeL, J::HandL),
            (-1.0, J::ArmR, J::ForeR, J::HandR),
        ] {
            let tag = if side > 0.0 { "l" } else { "r" };
            let reach = if l.v.woman { 0.215 } else { 0.24 };
            let upper = kit.joint(
                arm,
                torso,
                Vec3::new(side * reach, 0.48, 0.0),
                &format!("arm{tag}"),
                |p| {
                    limb(&mut p.matte, 0.3, 0.064, 0.05, 0.068, 0.008, l.cloth);
                },
            );
            let lower = kit.joint(fore, upper, Vec3::Y * -0.3, &format!("fore{tag}"), |p| {
                limb(&mut p.matte, 0.26, 0.05, 0.038, 0.056, 0.004, l.cloth);
                p.satin.prism(
                    Vec3::Y * -0.13,
                    Vec3::Y * -0.25,
                    0.058,
                    0.052,
                    10,
                    LEATHER,
                    0.0,
                );
                fist(p, Vec3::Y * -0.3, side, 1.0, l.cloth);
            });
            let grip = kit
                .commands
                .spawn((
                    Transform::from_translation(Vec3::Y * -0.31),
                    Visibility::default(),
                ))
                .id();
            kit.commands.entity(lower).add_child(grip);
            kit.rig.joints[hand as usize] = Some(grip);
            // Without its rest offset the grip would be posed at the elbow.
            kit.rig.rest[hand as usize] = Vec3::Y * -0.31;
        }
        let hand = kit.rig.joint(J::HandR).unwrap();
        kit.attach(
            hand,
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            "spear",
            |p| {
                p.matte
                    .prism(Vec3::Y * -0.7, Vec3::Y * 0.9, 0.02, 0.02, 6, WOOD, 0.0);
                p.satin
                    .prism(Vec3::Y * 0.78, Vec3::Y * 0.9, 0.024, 0.024, 6, LEATHER, 0.0);
                p.metal
                    .prism(Vec3::Y * 0.9, Vec3::Y * 1.14, 0.045, 0.0, 4, STEEL, 0.0);
                p.matte.prism(
                    Vec3::Y * 0.86,
                    Vec3::new(0.05, 0.7, 0.0),
                    0.012,
                    0.0,
                    3,
                    Vec3::new(0.6, 0.15, 0.1),
                    0.0,
                );
            },
        );
    } else {
        let neck = kit.joint(J::Neck, body, neck_at, "neck", |p| {
            let top = Vec3::new(0.0, 0.36, 0.3);
            let neck_coat = |dir: Vec3, at: Vec3| -> Vec3 {
                if quagga {
                    let wave = (at.y * 52.0 + at.z * 12.0).sin();
                    l.cloth.lerp(
                        Vec3::new(0.38, 0.21, 0.11),
                        ((wave - 0.05) * 3.0).clamp(0.0, 1.0),
                    )
                } else {
                    equine_hide(l.skin, style, dir)
                }
            };
            // The crest, throat and root form one angled neck. Its broad
            // root continues into the chest, covering the articulated join.
            use crate::sdf::{Sculpt, Shape::*};
            let mut neck_shape = Sculpt::default();
            neck_shape
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.045, -0.06), Vec3::new(0.125, 0.18, 0.165)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.065), Vec3::new(0.112, 0.19, 0.15)),
                    0.055,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.245, 0.19), Vec3::new(0.085, 0.15, 0.12)),
                    0.045,
                )
                .add(Ellipsoid(top, Vec3::new(0.074, 0.095, 0.09)), 0.035);
            neck_shape.mesh(
                &mut p.matte,
                Vec3::new(-0.17, -0.25, -0.25),
                Vec3::new(0.17, 0.49, 0.42),
                0.009,
                &|at, normal| {
                    if quagga && normal.z > 0.7 && at.y < 0.32 {
                        l.cloth
                    } else {
                        neck_coat(normal, at)
                    }
                },
            );
            let mane = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            // The mane: locks along the crest. A unicorn's falls long and
            // sweeps to one side; a quagga's stands up in a short brush.
            let locks = if style == Style::Unicorn { 20 } else { 18 };
            for k in 0..locks {
                let t = k as f32 / (locks - 1) as f32;
                // Along the crest, the top of the arched neck.
                let crest = Vec3::new(0.0, 0.06 + t * 0.36, -0.16 + t * 0.4);
                let root = neck_shape.onto(crest - Vec3::Z * 0.18, Vec3::Z);
                let shade = mane * (0.8 + 0.35 * rand(5, k));
                if style == Style::Unicorn {
                    let side = if k % 3 == 0 { -1.0 } else { 1.0 };
                    let len = 0.26 + 0.12 * rand(6, k);
                    let curl = 0.03 * (k as f32 * 1.3).sin();
                    // Each lock lifts off the crest, then falls close along the neck.
                    let path = [
                        root,
                        root + Vec3::new(side * 0.03, 0.05, -0.05),
                        root + Vec3::new(side * 0.08, 0.0, -0.09),
                        root + Vec3::new(side * 0.1 + curl, -len * 0.55, -0.1 + 0.05 * rand(7, k)),
                        root + Vec3::new(side * 0.09 - curl + 0.02 * rand(8, k), -len, -0.06),
                    ];
                    let r = 0.03 + 0.012 * rand(9, k);
                    tube(
                        &mut p.matte,
                        &path,
                        &[
                            (r, r * 0.7),
                            (r * 1.1, r * 0.8),
                            (r, r * 0.7),
                            (r * 0.6, r * 0.5),
                            (0.003, 0.003),
                        ],
                        shade,
                    );
                } else {
                    let path = [
                        root,
                        root + Vec3::new(0.0, 0.06, -0.035),
                        root + Vec3::new(0.0, 0.1, -0.06),
                    ];
                    tube(
                        &mut p.matte,
                        &path,
                        &[(0.016, 0.03), (0.014, 0.026), (0.002, 0.004)],
                        shade,
                    );
                }
            }
        });
        kit.joint(J::Snout, neck, Vec3::new(0.0, 0.4, 0.31), "head", |p| {
            let face = |normal: Vec3, at: Vec3| -> Vec3 {
                if quagga && (at.z * 30.0 - at.y * 20.0).sin() > 0.35 && at.z < 0.2 {
                    Vec3::new(0.38, 0.21, 0.11)
                } else if quagga {
                    l.cloth
                } else {
                    equine_hide(l.skin, style, normal)
                }
            };
            // A touch larger than life, as the renders draw it.
            p.set_transform(Affine3A::from_scale(Vec3::splat(1.15)));
            use crate::sdf::{Sculpt, Shape::*};
            let mut skull = Sculpt::default();
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.01, 0.03), Vec3::new(0.083, 0.095, 0.12)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.018, 0.16), Vec3::new(0.065, 0.061, 0.155)),
                    0.03,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.075, 0.16), Vec3::new(0.053, 0.039, 0.13)),
                    0.025,
                );
            for side in [-1.0f32, 1.0] {
                skull
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.052, -0.035, 0.03),
                            Vec3::new(0.039, 0.057, 0.077),
                        ),
                        0.025,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.061, 0.042, 0.057),
                            Vec3::new(0.027, 0.024, 0.043),
                        ),
                        0.016,
                    );
            }
            let eye_frames = animal_eye_sockets(
                &mut skull,
                Vec3::new(0.0, 0.034, 0.076),
                0.077,
                0.018,
                Vec3::new(0.92, 0.08, 0.38),
            );
            skull.mesh(
                &mut p.matte,
                Vec3::new(-0.12, -0.14, -0.11),
                Vec3::new(0.12, 0.13, 0.335),
                0.006,
                &|at, normal| face(normal, at),
            );
            let muzzle = if quagga { DARK * 3.0 } else { l.skin * 1.4 };
            p.matte.blob(
                Vec3::new(0.0, -0.08, 0.33),
                Vec3::new(0.062, 0.058, 0.05),
                2,
                0.0,
                muzzle,
            );
            p.matte.blob(
                Vec3::new(0.0, -0.125, 0.29),
                Vec3::new(0.04, 0.025, 0.045),
                3,
                0.0,
                muzzle * 0.9,
            );
            for side in [-1.0f32, 1.0] {
                p.matte.blob(
                    Vec3::new(side * 0.028, -0.07, 0.375),
                    Vec3::new(0.012, 0.016, 0.01),
                    0,
                    0.0,
                    DARK,
                );
                // Leaf-shaped ears, cupped forward, darker inside.
                let base = Vec3::new(side * 0.045, 0.08, -0.01);
                let ear = [
                    base,
                    base + Vec3::new(side * 0.01, 0.07, 0.0),
                    base + Vec3::new(side * 0.02, 0.14, -0.03),
                ];
                tube(
                    &mut p.matte,
                    &ear,
                    &[(0.034, 0.016), (0.03, 0.014), (0.004, 0.004)],
                    l.skin,
                );
                tube(
                    &mut p.matte,
                    &ear.map(|v| v + Vec3::Z * 0.008),
                    &[(0.022, 0.007), (0.019, 0.007), (0.002, 0.002)],
                    l.skin * 0.55,
                );
            }
            // The forelock spills over the brow.
            let lock = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            hair_tufts(
                p,
                Vec3::new(0.0, 0.1, 0.0),
                0.05,
                5,
                Vec3::new(0.0, -0.3, 1.0),
                lock,
                11,
            );
            animal_eyes(
                p,
                eye_frames,
                0.018,
                Vec3::new(0.44, 0.24, 0.09),
                l.skin,
                l.eyes,
            );
            if style == Style::Unicorn {
                p.accent = Some(Accent::Violet);
                let (a, b) = (Vec3::new(0.0, 0.09, 0.07), Vec3::new(0.0, 0.4, 0.28));
                p.metal
                    .prism(a, b, 0.034, 0.0, 10, Vec3::new(0.12, 0.1, 0.16), 0.0);
                for k in 0..4 {
                    let t = 0.12 + k as f32 * 0.2;
                    p.glow.torus(
                        a.lerp(b, t),
                        (b - a).normalize(),
                        0.034 * (1.0 - t) + 0.005,
                        0.005,
                        12,
                        3,
                        Vec3::ONE,
                    );
                }
                hair_tufts(
                    p,
                    Vec3::new(0.0, -0.12, 0.27),
                    0.025,
                    4,
                    Vec3::new(0.0, -1.0, -0.2),
                    l.cloth,
                    61,
                );
            }
        });
    }
    kit.joint(J::Tail, body, Vec3::new(0.0, 0.1, -0.54), "tail", |p| {
        let c = if style == Style::Unicorn {
            l.cloth
        } else {
            l.trim
        };
        tube(
            &mut p.matte,
            &[
                Vec3::ZERO,
                Vec3::new(0.0, -0.06, -0.06),
                Vec3::new(0.0, -0.14, -0.09),
            ],
            &[(0.05, 0.05), (0.04, 0.042), (0.03, 0.03)],
            l.skin,
        );
        // A flowing switch of hair: strands that fan and curl.
        let strands = if style == Style::Unicorn { 12 } else { 9 };
        for k in 0..strands {
            let a = (k as f32 - (strands - 1) as f32 * 0.5) * 0.11;
            let sway = (k as f32 * 1.7).sin() * 0.03;
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.1, -0.08),
                    Vec3::new(a * 0.12, -0.26, -0.15),
                    Vec3::new(a * 0.25 + sway, -0.42, -0.16),
                    Vec3::new(a * 0.32 + sway, -0.56, -0.12),
                ],
                &[(0.034, 0.034), (0.034, 0.03), (0.024, 0.02), (0.002, 0.002)],
                c * (0.8 + 0.4 * rand(13, k)),
            );
        }
    });
    let legs = [
        (
            J::FL,
            J::FLs,
            J::HoofFL,
            Vec3::new(0.12, -0.1, 0.38),
            "fl",
            true,
        ),
        (
            J::FR,
            J::FRs,
            J::HoofFR,
            Vec3::new(-0.12, -0.1, 0.38),
            "fr",
            true,
        ),
        (
            J::BL,
            J::BLs,
            J::HoofBL,
            Vec3::new(0.125, -0.1, -0.4),
            "bl",
            false,
        ),
        (
            J::BR,
            J::BRs,
            J::HoofBR,
            Vec3::new(-0.125, -0.1, -0.4),
            "br",
            false,
        ),
    ];
    for (thigh, shin, hoof, at, tag, front) in legs {
        let leg_color = if quagga { l.cloth } else { l.skin };
        let upper_end = Vec2::new(0.05, 0.058);
        let lower_root = Vec2::new(0.05, 0.056);
        let upper_scale = storybook_part(kit.rig.plan, thigh);
        let lower_scale = storybook_part(kit.rig.plan, shin);
        // A pivot-centered cover contains both scaled end rings as the lower
        // leg turns. Size it from those rings, not the larger muscle above.
        let joint_radius = (upper_end.max_element() * upper_scale.max_element())
            .max(lower_root.max_element() * lower_scale.max_element())
            + 0.0015;
        let joint_radii = Vec3::splat(joint_radius) / upper_scale;
        let root_overlap = lower_root.min_element() * 0.25;
        let upper = kit.joint(thigh, body, at, &format!("leg{tag}"), |p| {
            // A muscled upper leg tapering to a bony knee.
            let (top, mid) = if front { (0.125, 0.092) } else { (0.15, 0.11) };
            let path = if front {
                [
                    Vec3::new(0.0, 0.16, 0.0),
                    Vec3::new(0.0, -0.1, 0.015),
                    Vec3::new(0.0, -0.3, 0.0),
                    Vec3::Y * -0.42,
                ]
            } else {
                // The gaskin sweeps back and down to the hock.
                [
                    Vec3::new(0.0, 0.1, 0.02),
                    Vec3::new(0.0, -0.1, -0.06),
                    Vec3::new(0.0, -0.3, -0.06),
                    Vec3::Y * -0.42,
                ]
            };
            tube(
                &mut p.matte,
                &path,
                &[
                    (top * 0.6, top * 0.8),
                    (mid * 0.82, mid),
                    (0.056, 0.064),
                    (upper_end.x, upper_end.y),
                ],
                l.skin,
            );
            // Enough rings keep the small cover inside its silhouette margin.
            // The original offset knee/hock landmark remains visible below.
            let resolution = p.matte.res;
            p.matte.res = (18, 32);
            p.matte.blob(Vec3::Y * -0.42, joint_radii, 1, 0.0, l.skin);
            p.matte.res = resolution;
            let joint = if front {
                Vec3::new(0.0, -0.42, 0.012)
            } else {
                Vec3::new(0.0, -0.42, -0.03)
            };
            p.matte
                .blob(joint, Vec3::new(0.044, 0.052, 0.05), 1, 0.0, l.skin);
        });
        let lower = kit.joint(shin, upper, Vec3::Y * -0.42, &format!("shin{tag}"), |p| {
            // Keep the hidden overlap separate: tube paint uses arc length,
            // so inserting it into the cannon would shift the stocking fade.
            p.matte.tube(
                &[Vec3::Y * root_overlap, Vec3::ZERO],
                &[lower_root * 0.85, lower_root],
                12,
                &|_, _, _| l.skin,
            );
            // The stocking fades into the cannon bone instead of ending in
            // a hard sleeve. The fetlock and forward pastern carry the hoof.
            let radii = [
                lower_root,
                Vec2::new(0.035, 0.041),
                Vec2::new(0.043, 0.051),
                Vec2::new(0.038, 0.045),
                Vec2::new(0.045, 0.05),
            ];
            p.matte.tube(
                &[
                    Vec3::ZERO,
                    Vec3::Y * -0.2,
                    Vec3::Y * -0.34,
                    Vec3::new(0.0, -0.39, 0.012),
                    Vec3::new(0.0, -0.42, 0.02),
                ],
                &radii,
                12,
                &|_, _, t| l.skin.lerp(leg_color, ease((t - 0.08) / 0.28)),
            );
        });
        let hoof_transform = terminal_part_transform(kit.rig.plan, shin, hoof, EQUINE_ANKLE);
        kit.joint(hoof, lower, EQUINE_ANKLE, &format!("hoof{tag}"), |p| {
            // Its profile and neutral sole height stay unchanged. The ankle
            // articulation keeps the hoof level as the cannon rolls over it.
            p.set_transform(hoof_transform);
            // A flared hoof with a darker sole.
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.405, 0.018),
                    Vec3::new(0.0, -0.45, 0.026),
                    Vec3::new(0.0, -0.48, 0.034),
                ],
                &[(0.047, 0.051), (0.056, 0.062), (0.064, 0.072)],
                DARK * 2.4,
            );
            p.matte.blob(
                Vec3::new(0.0, -0.482, 0.034),
                Vec3::new(0.062, 0.006, 0.07),
                0,
                0.0,
                DARK * 1.4,
            );
        });
    }
}

/// A banded snake: a smooth tapering body laid along a travelling wave, a
/// wedge-shaped head with a jaw, and a tongue that flickers.
fn serpent(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let rattler = l.style == Style::Rattlesnake;
    for i in 0..SNAKE {
        let r = snake_segment_radius(i);
        let model = kit.part(&format!("seg{i}"), |p| {
            if i == 0 {
                // A low wedge grows continuously out of the neck. Brow and
                // jaw are part of the skull; the eyes sit inside its outline.
                use crate::sdf::{Sculpt, Shape::*};
                // A pit viper's head is a broad triangle behind its eyes.
                p.set_transform(Affine3A::from_scale(if rattler {
                    Vec3::new(1.5, 1.25, 1.3)
                } else {
                    Vec3::splat(1.25)
                }));
                let mut head = Sculpt::default();
                head.add(
                    Ellipsoid(Vec3::new(0.0, 0.0, 0.015), Vec3::new(0.078, 0.048, 0.105)),
                    0.0,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.012, 0.105),
                        Vec3::new(0.052, 0.029, 0.066),
                    ),
                    0.035,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.033, 0.06), Vec3::new(0.063, 0.016, 0.092)),
                    0.015,
                );
                for side in [-1.0f32, 1.0] {
                    head.add(
                        Ellipsoid(
                            Vec3::new(side * 0.05, 0.029, 0.049),
                            Vec3::new(0.028, 0.021, 0.04),
                        ),
                        0.018,
                    );
                }
                let eye_frames = animal_eye_sockets(
                    &mut head,
                    Vec3::new(0.0, 0.025, 0.068),
                    0.06,
                    0.016,
                    Vec3::new(0.78, 0.14, 0.61),
                );
                head.mesh(
                    &mut p.satin,
                    Vec3::new(-0.1, -0.065, -0.105),
                    Vec3::new(0.1, 0.075, 0.185),
                    0.0045,
                    &|at, normal| {
                        if normal.y < -0.25 && at.y < -0.02 {
                            l.trim
                        } else {
                            l.skin
                        }
                    },
                );
                for side in [-1.0f32, 1.0] {
                    p.matte.blob(
                        Vec3::new(side * 0.02, 0.001, 0.166),
                        Vec3::new(0.0045, 0.003, 0.003),
                        0,
                        0.0,
                        DARK,
                    );
                }
                animal_eyes(
                    p,
                    eye_frames,
                    0.016,
                    Vec3::new(0.86, 0.57, 0.1),
                    l.skin,
                    None,
                );
            } else if rattler && i >= SNAKE - 3 {
                // The rattle: stacked hollow segments, each a ridged ring.
                let k = (i - (SNAKE - 3)) as f32;
                let tone = Vec3::new(0.86, 0.74, 0.5) * (1.0 - 0.06 * k);
                for ring in 0..3 {
                    let z = (ring as f32 - 1.0) * 0.022;
                    p.satin.blob(
                        Vec3::new(0.0, 0.0, z),
                        Vec3::new(0.034 - k * 0.004, 0.026 - k * 0.003, 0.016),
                        ring,
                        0.0,
                        tone * (0.9 + 0.05 * ring as f32),
                    );
                }
            } else if rattler {
                // Dark diamonds outlined in cream down the olive back.
                let half_length = (r * 1.9).max(0.052);
                p.satin.blob_painted(
                    Vec3::ZERO,
                    Vec3::new(r, r * 0.75, half_length),
                    i as u64,
                    0.0,
                    &|normal, at| {
                        let phase = (i % 2) as f32 * half_length;
                        let diamond = at.x.abs() / (r * 0.85)
                            + ((at.z + phase).rem_euclid(half_length * 2.0) - half_length).abs()
                                / (half_length * 0.9);
                        if normal.y < -0.25 {
                            l.trim
                        } else if normal.y > -0.1 && diamond < 0.75 {
                            Vec3::new(0.24, 0.17, 0.1)
                        } else if normal.y > -0.1 && diamond < 0.95 {
                            l.cloth
                        } else {
                            l.skin
                        }
                    },
                );
            } else {
                // A painted belly and flush dorsal diamonds share the same
                // surface as the banded hide. No floating blocks on the spine.
                let band = (i / 2) % 2 == 1;
                let c = if band { l.cloth } else { l.skin };
                let half_length = (r * 1.9).max(0.052);
                p.satin.blob_painted(
                    Vec3::ZERO,
                    Vec3::new(r, r * 0.75, half_length),
                    i as u64,
                    0.0,
                    &|normal, at| {
                        let diamond = at.x.abs() / (r * 0.7) + at.z.abs() / (half_length * 0.6);
                        if normal.y < -0.25 {
                            l.trim
                        } else if i % 2 == 0 && normal.y > 0.35 && diamond < 1.0 {
                            l.skin * 0.68
                        } else {
                            c
                        }
                    },
                );
            }
        });
        let e = kit
            .commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| model.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
    // The tongue rides on the head and flicks out now and then.
    let head = kit.rig.segments[0];
    let tongue = kit.part("tongue", |p| {
        let red = Vec3::new(0.7, 0.12, 0.12);
        p.matte.prism(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 0.08),
            0.006,
            0.005,
            4,
            red,
            0.0,
        );
        for side in [-1.0f32, 1.0] {
            p.matte.prism(
                Vec3::new(0.0, 0.0, 0.08),
                Vec3::new(side * 0.015, 0.0, 0.11),
                0.004,
                0.0,
                3,
                red,
                0.0,
            );
        }
    });
    let e = kit
        .commands
        .spawn((Transform::from_xyz(0.0, -0.02, 0.15), Visibility::default()))
        .id();
    kit.commands.entity(e).with_children(|p| tongue.attach(p));
    kit.commands.entity(head).add_child(e);
    kit.rig.joints[J::Snout as usize] = Some(e);
    kit.rig.rest[J::Snout as usize] = Vec3::new(0.0, -0.02, 0.15);
}

/// Segments in a snake's body.
pub(super) const SNAKE: usize = 20;

/// Horizontal radius shared by the tapered mesh and its floor placement.
/// Body sections are three quarters as high as they are wide; the separately
/// sculpted head has its own low jaw profile.
pub(super) fn snake_segment_radius(index: usize) -> f32 {
    if index == 0 {
        0.08
    } else {
        let fraction = index as f32 / SNAKE as f32;
        0.093 * (1.0 - fraction).powf(0.72) + 0.005
    }
}

/// A crystal has broad shaded faces and a sharply terminating cap. Emission
/// belongs at the tip; a completely glowing cone loses all of its volume.
fn ice_shard(p: &mut Parts, root: Vec3, tip: Vec3, width: f32, color: Vec3) {
    let shoulder = root.lerp(tip, 0.68);
    let cap = root.lerp(tip, 0.9);
    p.satin
        .prism(root, shoulder, width * 0.55, width, 6, color, 0.0);
    p.satin
        .prism(shoulder, cap, width, width * 0.3, 6, color * 1.15, 0.0);
    p.glow.prism(cap, tip, width * 0.3, 0.0, 6, Vec3::ONE, 0.0);
}

/// Things that float: a burning wisp, a drifting knot of ice.
fn floater(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let aura = l.aura.unwrap_or(Accent::Ember);
    let core = kit.joint(J::Core, lift, Vec3::Y * 1.1, "core", |p| {
        p.accent = Some(aura);
        if l.style == Style::Ice {
            // Dark blue recesses and pale cyan planes keep the cluster
            // readable in daylight as well as against the dark dungeon.
            p.satin.smooth = false;
            p.satin.sides_mul = 1;
            p.satin.res = (5, 8);
            p.glow.smooth = false;
            p.glow.sides_mul = 1;
            p.satin.blob(
                Vec3::ZERO,
                Vec3::new(0.17, 0.205, 0.15),
                4,
                0.13,
                Vec3::new(0.12, 0.27, 0.4),
            );
            for k in 0..9 {
                let a = k as f32 * TAU / 9.0;
                let dir = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
                let color = Vec3::new(0.16, 0.43, 0.61).lerp(l.skin, 0.15 + 0.5 * rand(17, k));
                ice_shard(
                    p,
                    dir * 0.06,
                    dir * (0.3 + rand(k as u64, 3) * 0.15),
                    0.055 + 0.025 * rand(19, k),
                    color,
                );
            }
            eyes(
                p,
                Some(Accent::White),
                Vec3::new(0.0, 0.04, 0.15),
                0.06,
                0.02,
            );
            p.accent = Some(aura);
        } else {
            // Shaded outer petals frame the luminous heart and curling tongues.
            for k in 0..6 {
                let a = k as f32 * TAU / 6.0 + 0.3;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                tube(
                    &mut p.matte,
                    &[
                        out * 0.05 - Vec3::Y * 0.09,
                        out * 0.19 + Vec3::Y * 0.02,
                        out * 0.12 + Vec3::Y * 0.17,
                    ],
                    &[(0.05, 0.02), (0.06, 0.025), (0.004, 0.004)],
                    Vec3::new(0.85, 0.22, 0.035),
                );
            }
            for (k, h) in [(0, 0.52f32), (1, 0.36), (2, 0.4)] {
                let a = k as f32 * 2.2;
                let out = Vec3::new(a.cos(), 0.0, a.sin()) * 0.04;
                let curl = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.06;
                tube(
                    &mut p.glow,
                    &[
                        out,
                        out * 1.4 + Vec3::Y * h * 0.4 + curl * 0.5,
                        out + Vec3::Y * h * 0.75 - curl * 0.3,
                        Vec3::Y * h + curl,
                    ],
                    &[(0.075, 0.05), (0.06, 0.04), (0.03, 0.02), (0.003, 0.003)],
                    Vec3::ONE,
                );
            }
        }
    });
    if l.style == Style::Wisp {
        // A brighter golden heart inside the flames.
        kit.attach(core, Transform::IDENTITY, "heart", |p| {
            p.accent = Some(Accent::Amber);
            p.glow
                .blob(Vec3::Y * 0.02, Vec3::splat(0.085), 1, 0.0, Vec3::ONE);
        });
    }
    if l.style == Style::Ice {
        for (side, arm) in [(1.0f32, J::ArmL), (-1.0, J::ArmR)] {
            kit.joint(
                arm,
                core,
                Vec3::new(side * 0.42, -0.1, 0.0),
                &format!("shard{side}"),
                |p| {
                    p.accent = Some(aura);
                    p.satin.smooth = false;
                    p.satin.sides_mul = 1;
                    p.glow.smooth = false;
                    p.glow.sides_mul = 1;
                    for k in 0..4 {
                        let dir = Vec3::new(side * 0.3, -1.0, 0.2 * (k as f32 - 1.5)).normalize();
                        ice_shard(
                            p,
                            Vec3::new(0.0, 0.0, (k as f32 - 1.5) * 0.022),
                            dir * (0.32 + k as f32 * 0.04),
                            0.045 + k as f32 * 0.004,
                            Vec3::new(0.22, 0.5, 0.69).lerp(l.skin, k as f32 * 0.18),
                        );
                    }
                },
            );
        }
    }
    let mote = kit.part("mote", |p| {
        p.accent = Some(aura);
        p.glow
            .blob(Vec3::ZERO, Vec3::splat(0.035), 0, 0.0, Vec3::ONE);
    });
    for _ in 0..6 {
        let e = kit
            .commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| mote.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
}
