use super::*;

/// Everything a biped's parts are cut from: the wearer, what it wears and
/// the colour of each layer, resolved once before any part is built.
pub(super) struct Figure {
    /// The wearer's look.
    pub(super) look: Look,
    pub(super) style: Style,
    pub(super) body: HumanoidBody,
    pub(super) fem: bool,
    pub(super) build: f32,
    pub(super) shoulders: f32,
    pub(super) waist: f32,
    pub(super) arm_len: f32,
    pub(super) outfit: Outfit,
    pub(super) heraldry: Heraldry,
    /// Dressed in its kind's own outfit.
    pub(super) native: bool,
    pub(super) fitted_robe: bool,
    pub(super) boots: Option<super::outfit::Boots>,
    pub(super) shoes: Option<Shoes>,
    pub(super) boot_shape: BootShape,
    pub(super) blocky: bool,
    pub(super) furry: bool,
    pub(super) cuirass: bool,
    /// The colour of each layer.
    pub(super) tunic: Vec3,
    pub(super) pants: Vec3,
    pub(super) bare_arms: bool,
    pub(super) barefoot: bool,
    pub(super) trousered: bool,
    pub(super) boot: Vec3,
    pub(super) sleeve: Vec3,
    pub(super) fore_color: Vec3,
    pub(super) styled: Option<super::outfit::Sleeve>,
    pub(super) hand_color: Vec3,
    pub(super) trim_neck: bool,
    /// Torso shells, innermost first.
    pub(super) shell_shape: Option<crate::sdf::Sculpt>,
    pub(super) native_plate: Option<crate::sdf::Sculpt>,
    pub(super) armor_item: Option<super::outfit::BodyItem>,
    pub(super) equipped_plate: Option<crate::sdf::Sculpt>,
    /// What a belt or waistband clears.
    pub(super) waist_support: crate::sdf::Sculpt,
}

impl Figure {
    pub(super) fn resolve(body: HumanoidBody, look: &Look) -> Self {
        let l = look.clone();
        let style = l.style;
        let fem = body.woman;
        let b = body.build;
        let (shoulders, waist) = (body.shoulders, body.waist);
        let arm_len = body.arm_length;
        // What it wears, slot by slot: its kind's own outfit unless it was
        // dressed in another. A kind's own trimmings (lacing, straps, rags,
        // charms) come only with its own outfit; another outfit brings its own.
        let composition = Composition::resolve(&l);
        let outfit = composition.outfit;
        let heraldry = outfit.trimmings.heraldry.unwrap_or(Heraldry {
            field: l.cloth,
            edge: GILT,
            emblem: CREAM,
            mark: l.v.pattern,
        });
        let native = l.outfit.is_none();
        let fitted_robe = outfit.robe.is_some();
        let boots = match outfit.feet {
            Feet::Boots(boots) => Some(boots),
            Feet::Bare(_) | Feet::Shoes(_) => None,
        };
        let shoes: Option<Shoes> = match outfit.feet {
            Feet::Shoes(shoes) => Some(shoes),
            _ => None,
        };
        // Footwear is fitted to the resolved wearer.
        let boot_shape = BootShape::fitted(
            b,
            if boots.is_some_and(|boots| boots.tall) {
                -0.075
            } else {
                -0.24
            },
        );
        let blocky = style == Style::Guardian;
        let furry = style == Style::Yeti;
        let cuirass = outfit.outer == Some(Outer::Cuirass);
        // The colour of each layer.
        let tunic = outfit.tunic;
        let pants = outfit.pants(l.skin);
        let bare_arms = outfit.arms.bare();
        let barefoot = matches!(outfit.feet, Feet::Bare(_));
        let trousered = matches!(outfit.legs, Legs::Trousers(_));
        let boot = outfit.boot(l.skin);
        let sleeve = outfit.sleeve(l.skin);
        let fore_color = outfit.forearm(l.skin);
        let styled = match outfit.arms {
            Arms::Styled(sleeve) => Some(sleeve),
            _ => None,
        };
        let hand_color = outfit.hands.resolve(l.skin);
        let trim_neck = !native
            || !matches!(
                style,
                Style::Sentinel | Style::Hobgoblin | Style::Zombie | Style::Orc | Style::Troll
            );
        let shell_shape = outfit_shell(body, &outfit, trim_neck);
        let native_plate = cuirass.then(|| {
            let mut plate = torso_garment(body, TorsoMaterial::Plate, trim_neck);
            plate
                .carve(crate::sdf::Shape::Beyond(Vec3::Y, 0.525), 0.004)
                .carve(crate::sdf::Shape::Beyond(Vec3::NEG_Y, -0.012), 0.0);
            if let Some(cut) = NecklineCut::resolve(outfit.bodice.cut, body) {
                plate.carve(cut.opening(body, 0.525, 0.008), 0.006);
            }
            plate
        });
        // Every torso armour shares one fitted shell; its recipe dresses the surface.
        let armor_item = composition.body;
        let equipped_plate = armor_item.map(|_| {
            armor_parts::shell(
                body,
                native_plate.as_ref().or(shell_shape.as_ref()),
                trim_neck,
                if cuirass { EMBLEM_FIELD_OFFSET } else { 0.0 },
            )
        });
        let mut waist_support =
            torso_shape(body, cuirass.then_some(TorsoMaterial::Plate), trim_neck);
        if let Some(shell) = &shell_shape {
            waist_support.union(shell.clone(), 0.0);
        }
        if let Some(plate) = &equipped_plate {
            waist_support.union(plate.clone(), 0.0);
        }
        Self {
            look: l,
            style,
            body,
            fem,
            build: b,
            shoulders,
            waist,
            arm_len,
            outfit,
            heraldry,
            native,
            fitted_robe,
            boots,
            shoes,
            boot_shape,
            blocky,
            furry,
            cuirass,
            tunic,
            pants,
            bare_arms,
            barefoot,
            trousered,
            boot,
            sleeve,
            fore_color,
            styled,
            hand_color,
            trim_neck,
            shell_shape,
            native_plate,
            armor_item,
            equipped_plate,
            waist_support,
        }
    }
}

/// Upright figures: people, the dead, constructs and the yeti.
///
/// The body is shaped rather than stacked: a chest that narrows to the
/// waist, shoulders, a skirt over the hips, tapered limbs with knees and
/// elbows, calves, hands with thumbs, cuffed boots. Clothing is layered in
/// separate colours so a figure reads at a glance even from above.
pub(super) fn biped(kit: &mut Kit, lift: Entity, look: &Look) {
    let body = kit
        .rig
        .humanoid
        .expect("biped anatomy resolved before generation");
    let figure = Figure::resolve(body, look);
    let &Figure { body, outfit, .. } = &figure;
    let hips = kit.joint(J::Hips, lift, body.hips, "hips", |p| hip_parts(p, &figure));
    skirts(kit, &figure, hips);
    let torso = kit.joint(J::Torso, hips, body.torso, "torso", |p| {
        torso_parts(p, &figure)
    });
    let head = kit.joint(J::Head, torso, body.head, "head", |p| {
        head_parts(p, &figure)
    });
    arms(kit, &figure, torso);
    legs(kit, &figure, hips);
    loose_hair(kit, &figure, head);
    garment_details::spawn(kit, outfit.details, body, hips);
    cloak(kit, &figure, torso);
}

/// The pelvis, seat and lower garments that ride the hips.
fn hip_parts(p: &mut Parts, figure: &Figure) {
    let &Figure {
        look: ref l,
        style,
        body,
        outfit,
        heraldry,
        native,
        fitted_robe,
        blocky,
        furry,
        cuirass,
        tunic,
        ref waist_support,
        ..
    } = figure;
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
    if furry {
        p.matte.blob(
            Vec3::ZERO,
            body.pelvis_radii(),
            3,
            0.05,
            outfit.seat(l.skin),
        );
    } else {
        let seat = outfit.seat(l.skin);
        let high_cut = matches!(outfit.legs, Legs::Bare(_));
        let r = body.pelvis_radii();
        // Over bare legs the garment is cut high: each leg opening
        // rises from the crotch to the hip bone, by its share of the
        // pelvis's half-width.
        let leg_line = |x: f32| -0.075 * body.build + (x - 0.22).max(0.0).powf(1.3) * 0.24;
        pelvis_shape(body).mesh(
            &mut p.matte,
            -r * 1.15 - Vec3::Y * 0.02,
            r * 1.15 + Vec3::Y * 0.1,
            0.0085,
            &|at, _| {
                let x = at.x.abs() / r.x;
                if high_cut && x > 0.22 && at.y < leg_line(x) {
                    l.skin
                } else {
                    seat
                }
            },
        );
        // Her seat gives as she moves, as her chest does, unless plate
        // holds it.
        if body.woman && outfit.legs != Legs::Plate && style != Style::Zombie {
            let give = if high_cut { 0.7 } else { 0.55 };
            for (center, radii) in seat_masses(body) {
                p.soft.push(crate::props::SoftRegion {
                    center,
                    radii,
                    facing: -1.0,
                    give,
                });
            }
        }
        if high_cut {
            // A sewn binding finishes each leg opening, front and back.
            for front in [true, false] {
                for side in [-1.0f32, 1.0] {
                    let at = |x: f32| (Vec2::new(side * x * r.x, leg_line(x)), 0.01);
                    fitted_ribbon(
                        &mut p.matte,
                        &pelvis_shape(body),
                        &[0.2, 0.35, 0.5, 0.62, 0.74, 0.84, 0.92].map(at),
                        front,
                        0.003,
                        outfit.bodice.edge,
                    );
                }
            }
        }
    }
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
    if native && style == Style::Zombie {
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
    if cuirass {
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
                heraldry.edge,
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
                heraldry.field,
            );
        }
    }
    let belt = super::robe::belt_on(body, BELT_CENTER_Y, waist_support);
    // Caster cloth and its band already share a dedicated hip-frame rig.
    if outfit.belt && fitted_robe {
        waist_parts::belt(p, body, waist_support);
    }
    if let Some(worn) = outfit
        .trimmings
        .waist
        .filter(|w| fitted_robe || w.attachment() == J::Hips)
    {
        accessory_parts::waist(p, worn, body, waist_support);
    }
    if let Some(robe) = outfit.robe {
        // The knot and its surface-bound tails share the garment/belt
        // frame. Fit depth to that belt while retaining the authored knot.
        let x = 0.06;
        let front = belt.y * (1.0 - (x / belt.x).powi(2)).max(0.0).sqrt();
        p.matte.blob(
            Vec3::new(x, 0.11, front + 0.01),
            Vec3::new(0.035, 0.03, 0.02),
            1,
            0.0,
            robe.trim * 0.8,
        );
    }
    if let Some(pouch) = outfit.pouch {
        waist_parts::pouch(&mut p.satin, body, waist_support, pouch.color);
    }
}

/// The chest, bust, waist and every garment layer on the torso.
fn torso_parts(p: &mut Parts, figure: &Figure) {
    let &Figure {
        look: ref l,
        style,
        body,
        build: b,
        shoulders,
        waist,
        outfit,
        heraldry,
        fitted_robe,
        cuirass,
        tunic,
        bare_arms,
        trim_neck,
        ref shell_shape,
        ref native_plate,
        armor_item,
        ref equipped_plate,
        ref waist_support,
        ..
    } = figure;
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
    use crate::sdf::Sculpt;
    let (sh, w) = (shoulders * b, waist * b);
    let neckline = NecklineCut::resolve(outfit.bodice.cut, body);
    let mut body_shape = torso_shape(body, None, trim_neck);
    // The tunic painted on the body carries her chest as it is made to.
    if let (Some(bust), Some(share)) = (body.bust, outfit.bodice.support.share()) {
        bust_underbridge(&mut body_shape, &bust, share, 0.0, 0.0);
    }
    let body_shape = body_shape;
    let bare_shoulders = bare_arms;
    let skin_patch = |at: Vec3, normal: Vec3| {
        if style == Style::Zombie && normal.z > 0.35 {
            let tear = ((at.x - 0.055 * b) / (0.068 * b)).powi(2) + ((at.y - 0.25) / 0.105).powi(2);
            let wound = ((at.x + 0.1 * b) / (0.043 * b)).powi(2) + ((at.y - 0.38) / 0.055).powi(2);
            if tear < 1.0 + 0.12 * (at.y * 93.0).sin() {
                return Some(l.skin * 0.85);
            }
            if wound < 1.0 {
                return Some(Vec3::new(0.35, 0.12, 0.1));
            }
        }
        None
    };
    body_shape.mesh_partitioned(
        &mut p.matte,
        Vec3::new(-0.38, -0.06, -0.21) * b.max(1.0),
        Vec3::new(0.38, 0.64, 0.24) * b.max(1.0),
        0.0085,
        &|at, normal| {
            if outfit.bodice.cover == Cover::Bikini {
                // Each cup's rim is bound in a darker edge.
                return match bikini_cover(body, at) {
                    Some(rim) if rim > 0.86 => outfit.bodice.edge,
                    Some(_) => tunic * Sculpt::weave(at),
                    None => l.skin,
                };
            }
            let bare = !bodice_covers(body, outfit.bodice, bare_shoulders, at);
            // Women's bodices plunge down the sternum, showing the upper
            // chest and the valley between.
            if neckline.is_some_and(|cut| cut.open(at)) {
                return l.skin;
            }
            if let Some(pigment) = skin_patch(at, normal) {
                return pigment;
            }

            if bare {
                l.skin
            } else {
                let ground = match outfit.bodice.pattern {
                    Pattern::Brocade { .. } => tunic,
                    pattern => pattern.paint(tunic, at),
                };
                ground * Sculpt::weave(at)
            }
        },
        Some(&|at| {
            if outfit.bodice.cover == Cover::Bikini {
                bikini_cover(body, at).is_some()
            } else {
                bodice_covers(body, outfit.bodice, bare_shoulders, at)
                    && !neckline.is_some_and(|cut| cut.open(at))
            }
        }),
    );
    if let Pattern::Brocade { thread, motif } = outfit.bodice.pattern {
        if outfit.bodice.cover != Cover::Bikini {
            embroidery_parts::bodice(
                &mut p.matte,
                &body_shape,
                body.build,
                motif,
                thread,
                &|at, normal| {
                    bodice_covers(body, outfit.bodice, bare_shoulders, at)
                        && !neckline.is_some_and(|cut| cut.open(at))
                        && skin_patch(at, normal).is_none()
                },
            );
        }
    }
    // The free edges of cropped, strapless and one-shoulder bodices
    // are bound like any other.
    let bound = outfit.bodice.edge;
    if let Cover::Crop(hem) = outfit.bodice.cover {
        let fold = body.bust.map_or(0.27, |bust| bust.fold);
        for front in [true, false] {
            let path: Vec<(Vec2, f32)> = (0..=12)
                .map(|k| {
                    let x = (k as f32 / 12.0 - 0.5) * 0.36 * sh;
                    let z = if front { 0.1 } else { -0.1 };
                    let reach = hem.reach(x.atan2(z));
                    (
                        Vec2::new(x, fold - 0.03 - 0.04 * (1.0 - reach) + 0.004),
                        0.009,
                    )
                })
                .collect();
            fitted_ribbon(&mut p.matte, &body_shape, &path, front, 0.004, bound);
        }
    }
    match outfit.bodice.straps {
        Straps::Strapless | Straps::Sweetheart => {
            let dip = if outfit.bodice.straps == Straps::Sweetheart {
                0.55
            } else {
                0.1
            };
            neckline_parts::bind(p, body, &body_shape, dip, bound);
        }
        Straps::OneShoulder => {
            for front in [true, false] {
                let path: Vec<(Vec2, f32)> = (0..=12)
                    .map(|k| {
                        let x = -0.1 * sh + k as f32 / 12.0 * 0.27 * sh;
                        (Vec2::new(x, one_shoulder_top(body, x) - 0.004), 0.009)
                    })
                    .collect();
                fitted_ribbon(&mut p.matte, &body_shape, &path, front, 0.004, bound);
            }
        }
        Straps::Panels { width } => panel_parts::bind(p, &body_shape, body, width, bound),
        Straps::Halter | Straps::Shoulders => {}
    }
    if let (Cover::Bikini, Some(bust)) = (outfit.bodice.cover, body.bust) {
        // Halter strings rise from the top of each cup to the neck.
        for side in [-1.0f32, 1.0] {
            let (center, radii) = bust.mass(side, 0.0);
            let cup = Vec2::new(
                side * (center.x - 0.15 * radii.x),
                center.y + radii.y * 1.15,
            );
            let neck = Vec2::new(side * 0.055 * b, 0.53);
            fitted_ribbon(
                &mut p.matte,
                &body_shape,
                &[0.0, 0.33, 0.66, 1.0].map(|t| (cup.lerp(neck, t), 0.007)),
                true,
                0.003,
                tunic * 0.8,
            );
            // A bound rim traced round the cup's own outline.
            let rim: Vec<(Vec2, f32)> = (0..=16)
                .map(|k| {
                    let a = k as f32 / 16.0 * TAU;
                    let at = Vec2::new(
                        center.x + a.cos() * radii.x * 1.12 * 0.95,
                        center.y + 0.12 * radii.y + a.sin() * radii.y * 1.28 * 0.95,
                    );
                    (at, 0.008)
                })
                .collect();
            fitted_ribbon(
                &mut p.matte,
                &body_shape,
                &rim,
                true,
                0.003,
                outfit.bodice.edge,
            );
        }
    }
    if let Some(cut) = neckline.filter(|cut| cut.keyhole) {
        let rim: Vec<(Vec2, f32)> = cut
            .keyhole_rim()
            .into_iter()
            .map(|at| (at, 0.009))
            .collect();
        fitted_ribbon(
            &mut p.matte,
            &body_shape,
            &rim,
            true,
            0.004,
            outfit.bodice.edge,
        );
    }
    if let Some(cut) = neckline.filter(|cut| !cut.keyhole) {
        // A sewn edge finishes the neckline on both sides, following
        // the chest's upper slope to the collarbones.
        let edge = outfit.bodice.edge;
        for side in [-1.0f32, 1.0] {
            let at = |t: f32| {
                let y = cut.bottom - 0.004 + (cut.collar - cut.bottom + 0.004) * t;
                (Vec2::new(side * cut.edge(y.max(cut.bottom)), y), 0.011)
            };
            let mut path: Vec<(Vec2, f32)> =
                [1.0, 0.8, 0.6, 0.4, 0.2, 0.0].into_iter().map(at).collect();
            if cut.wrap && side > 0.0 {
                // Her left panel crosses over beneath the point and
                // runs down to the tie at the right of her waist.
                let waist = Vec2::new(-0.11 * sh, 0.1);
                let point = path[path.len() - 1].0;
                for t in [0.25f32, 0.5, 0.75, 1.0] {
                    let along = point.lerp(waist, t);
                    // Bowed a little, as the panel wraps the rib cage.
                    let bow = (std::f32::consts::PI * t).sin() * 0.012;
                    path.push((along + Vec2::new(0.0, -bow), 0.011));
                }
            }
            fitted_ribbon(&mut p.matte, &body_shape, &path, true, 0.004, edge);
        }
    }
    if let (Some(cut), Some(cord)) = (neckline, outfit.bodice.lacing) {
        // Laces cross the lower plunge between eyelets on its edges,
        // drawn straight across the valley rather than into it.
        let rows = 4;
        let top = cut.bottom + (cut.collar - cut.bottom) * 0.55;
        let eyelet = |side: f32, k: usize| {
            let y = cut.bottom + 0.016 + (top - cut.bottom - 0.016) * k as f32 / rows as f32;
            let at = torso_surface(&body_shape, side * (cut.edge(y) + 0.007), y, true);
            at + Vec3::Z * 0.004
        };
        for k in 0..=rows {
            for side in [-1.0f32, 1.0] {
                p.metal
                    .blob(eyelet(side, k), Vec3::splat(0.0045), 0, 0.0, GILT * 0.8);
            }
        }
        for k in 0..rows {
            for side in [-1.0f32, 1.0] {
                tube(
                    &mut p.matte,
                    &[eyelet(side, k), eyelet(-side, k + 1)],
                    &[(0.0032, 0.0032), (0.0032, 0.0032)],
                    cord,
                );
            }
        }
    }
    // Her chest gives as she moves, less under structured leather and
    // not at all under plate; a fuller chest gives more.
    let support = if let Some(item) = armor_item {
        if item.flexible() {
            Support::Structured.give()
        } else {
            0.0
        }
    } else {
        match outfit.outer {
            Some(Outer::Cuirass) => 0.0,
            Some(Outer::Shell { leather: true, .. }) => Support::Structured.give(),
            _ => 1.0,
        }
        .min(outfit.bodice.support.give())
    };
    if let Some(bust) = body.bust.filter(|_| support > 0.0) {
        let give = support * (0.5 + 0.6 * bust.volume);
        for side in [-1.0f32, 1.0] {
            let (center, radii) = bust.mass(side, 0.0);
            p.soft.push(crate::props::SoftRegion {
                center,
                radii,
                facing: 1.0,
                give,
            });
        }
    }
    if let Some(edge) = outfit
        .armhole_edge
        .filter(|_| bare_arms && outfit.bodice.straps == Straps::Shoulders)
    {
        // A sleeveless bodice ends in sewn edges where the shoulders
        // and arms are bare, front and back.
        for front in [true, false] {
            for side in [-1.0f32, 1.0] {
                let at = |y: f32| {
                    let x = armhole_edge(body, y, if front { 1.0 } else { 0.0 });
                    (Vec2::new(side * x, y), 0.011)
                };
                fitted_ribbon(
                    &mut p.matte,
                    &body_shape,
                    &[at(0.425), at(0.445), at(0.47), at(0.5)],
                    front,
                    0.004,
                    edge,
                );
            }
        }
    }
    // This is the same resolved surface used by the lower seam.
    let shell = shell_shape.as_ref();
    if let (
        Some(shell),
        Some(Outer::Shell {
            color,
            top,
            quilted,
            underbust,
            ..
        }),
    ) = (shell, outfit.outer)
    {
        let top = shell_top(body, top, underbust);
        let c = color;
        // Padded cloth is matte; leather and fine cloth keep their sheen.
        let builder = if quilted { &mut p.matte } else { &mut p.satin };
        shell.mesh(
            builder,
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
                // A gambeson is stitched in diamonds, each cell padded.
                let quilting = if quilted {
                    let (u, v) = ((at.x + at.y) / 0.045, (at.x - at.y) / 0.045);
                    let (du, dv) = (u - u.round(), v - v.round());
                    if du.abs() < 0.07 || dv.abs() < 0.07 {
                        0.72
                    } else {
                        1.0 - 0.12 * (du.abs() + dv.abs())
                    }
                } else {
                    1.0
                };
                c * seam * quilting * (1.0 + 0.03 * (at.y * 57.0 + at.x * 31.0).sin())
            },
        );
        if underbust {
            // A corset laces up its front between gilt eyelets.
            let rows = 5;
            let eyelet = |side: f32, k: usize| {
                let y = 0.07 + (top - 0.095) * k as f32 / rows as f32;
                torso_surface(shell, side * 0.022 * b, y, true) + Vec3::Z * 0.003
            };
            for k in 0..=rows {
                for side in [-1.0f32, 1.0] {
                    p.metal
                        .blob(eyelet(side, k), Vec3::splat(0.0045), 0, 0.0, GILT * 0.8);
                }
            }
            for k in 0..rows {
                for side in [-1.0f32, 1.0] {
                    tube(
                        &mut p.matte,
                        &[eyelet(side, k), eyelet(-side, k + 1)],
                        &[(0.0032, 0.0032), (0.0032, 0.0032)],
                        CREAM * 0.8,
                    );
                }
            }
        }
    }
    // Where something sits on the front (or back) of the outer garment.
    // Resolve every structural layer once. Rendering and attachments
    // use the same plate shape instead of tracing the naked body under it.
    let plate = &native_plate;
    let garment = plate.as_ref().or(shell).unwrap_or(&body_shape);
    let mut attachment_support = torso_layer_support(&body_shape, garment);
    if let Some(armor) = &equipped_plate {
        attachment_support.union(armor.clone(), 0.0);
    }
    let garment = &attachment_support;
    let accessory_overlay = if plate.is_some() {
        EMBLEM_FIELD_OFFSET
    } else {
        0.0
    };
    if cuirass {
        // The separate gorget belongs to the articulated plate armour.
        p.matte.torus(
            Vec3::Y * 0.555,
            Vec3::Y,
            0.07,
            0.022,
            12,
            4,
            outfit.collar_edge,
        );
    } else if !bare_shoulders
        && !matches!(outfit.bodice.cover, Cover::Bare | Cover::Bikini)
        && matches!(outfit.bodice.straps, Straps::Shoulders | Straps::Halter)
    {
        fitted_band(
            &mut p.matte,
            &body_shape,
            0.535,
            0.552,
            0.0015,
            outfit.collar_edge,
        );
    }
    // A soft cowl or scarf gathered round the neck and draped over the shoulders.
    if let Some(NeckDrape { color, tail, .. }) = outfit.neck_drape {
        p.set_transform(Affine3A::from_translation(Vec3::new(0.0, 0.515, -0.012)));
        draped_cloth(
            &mut p.matte,
            ClothSurface::new(
                SkirtCut::straight(Vec2::new(0.087, 0.078) * b, Vec2::new(0.15, 0.118) * b),
                0.045,
                8,
                -0.025,
                u64::from(l.v.pattern),
            ),
            color,
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

    if let Some(cuirass) = &plate {
        // One fitted cuirass follows the rib cage and waist. Cloth
        // is an open panel laid on its surface, never a second torso
        // intersecting the metal or several circular gold bands.
        cuirass.mesh(
            &mut p.metal,
            Vec3::new(-0.34, -0.025, -0.23) * b.max(1.0),
            Vec3::new(0.34, 0.55, 0.255) * b.max(1.0),
            0.008,
            &|_, normal| STEEL * (0.92 + 0.08 * normal.z.abs()),
        );
        fitted_band(&mut p.metal, cuirass, 0.018, 0.034, 0.004, GILT);
        if let Some(cut) = neckline {
            // A gilt rolled edge finishes the lowered V.
            for side in [-1.0f32, 1.0] {
                let at = |t: f32| {
                    let y = cut.bottom + (0.515 - cut.bottom) * t;
                    (Vec2::new(side * (cut.edge(y) + 0.007), y), 0.019)
                };
                fitted_ribbon(
                    &mut p.metal,
                    cuirass,
                    &[0.02, 0.2, 0.4, 0.6, 0.8, 1.0].map(at),
                    true,
                    0.003,
                    GILT,
                );
            }
        }
        for front in [true, false] {
            // Below a low-cut front the tabard rises only to a point
            // beneath the plunge.
            let top = match neckline.filter(|_| front) {
                Some(cut) => cut.bottom - 0.03,
                None => 0.512,
            };
            let rise = |y: f32| 0.035 + (y - 0.035) * (top - 0.035) / (0.512 - 0.035);
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
            let edge = edge.map(|(y, x)| (rise(y), x));
            let cloth = cloth.map(|(y, x)| (rise(y), x));
            fitted_panel(&mut p.matte, cuirass, &edge, front, 0.009, heraldry.edge);
            fitted_panel(
                &mut p.matte,
                cuirass,
                &cloth,
                front,
                TABARD_SURFACE_OFFSET,
                heraldry.field,
            );
            fitted_emblem(
                &mut p.matte,
                cuirass,
                Vec2::new(0.0, rise(0.31)),
                0.075 * b * ((top - 0.035) / 0.477).sqrt(),
                front,
                heraldry.emblem,
                heraldry.field,
                heraldry.mark,
            );
        }
    }
    if let Some(worn) = outfit.trimmings.neck {
        accessory_parts::neck_collar(
            p,
            worn,
            b,
            if equipped_plate.is_some() {
                garment
            } else {
                &body_shape
            },
        );
    }
    if let Some(baldric) = outfit.baldric {
        fitted_baldric(p, garment, body, baldric, accessory_overlay);
    }
    if let (Some(item), Some(armor)) = (armor_item, &equipped_plate) {
        armor_parts::emit(p, armor, body, item);
    }
    if let Some(ornament) = outfit.torso_ornament {
        ornament_parts::emit(p, ornament, body, garment, &body_shape, neckline);
    }
    let fitted_waist = outfit
        .trimmings
        .waist
        .filter(|w| w.attachment() == J::Torso);
    if !fitted_robe && (outfit.belt || fitted_waist.is_some()) {
        // Every fitted waist piece follows the torso surface it encloses.
        // Continuous robe bands retain their dedicated hip/robe frame.
        p.set_transform(reparent_mesh_transform(
            Plan::Biped,
            J::Hips,
            J::Torso,
            body.torso,
        ));
        if outfit.belt {
            waist_parts::belt(p, body, waist_support);
        }
        if let Some(worn) = fitted_waist {
            accessory_parts::waist(p, worn, body, waist_support);
        }
        p.set_transform(Affine3A::IDENTITY);
    }
}

/// The head, face, hair and headwear.
fn head_parts(p: &mut Parts, figure: &Figure) {
    let &Figure {
        look: ref l,
        style,
        fem,
        build: b,
        outfit,
        blocky,
        furry,
        cuirass,
        ..
    } = figure;
    let neck = if furry { l.cloth } else { l.skin };
    let slim = neck_slim(fem);
    if blocky || furry || cuirass {
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
        if let Some(worn) = outfit.trimmings.neck {
            accessory_parts::neck_band(p, worn, b, slim);
        }
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
                    let muzzle = (1.0 - (at.x / 0.07).powi(2) - ((at.y - 0.069) / 0.045).powi(2))
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
                let (at, normal) = torso_surface_frame(&face, Vec2::new(side * 0.013, 0.114), true);
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
            human_face(p, l, !matches!(style, Style::Hobgoblin | Style::Troll));
            if shows_hair(l) {
                hair(p, l, !loose_fall(l));
            }
            if (l.v.beard
                && matches!(
                    style,
                    Style::Hero | Style::Caster | Style::Scout | Style::Common | Style::Zombie
                ))
                || (style == Style::Leprechaun && !fem)
            {
                beard(p, hair_color(l));
            }
            // What the outfit puts on the head.
            match outfit.head {
                Headwear::Helm => {
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
                        braid(p, Vec3::new(0.0, 0.05, -0.12), hair_color(l));
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
                Headwear::HoodDown(hood) => {
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
                            Ellipsoid(Vec3::new(0.0, 0.03, -0.1), Vec3::new(0.085, 0.045, 0.04)),
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
                Headwear::Hood {
                    color: hood,
                    pointed,
                } => {
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
                            Ellipsoid(Vec3::new(0.0, -0.03, -0.09), Vec3::new(0.16, 0.05, 0.09)),
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
                        let c = hair_color(l);
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
                Headwear::WingedHelm { metal, feathers } => {
                    headwear_parts::winged_helm(p, metal, feathers);
                    // The crown is tucked under the cap; a braid emerges
                    // below its rear rim, preserving the wearer's hair color.
                    if fem && grows_scalp_hair(l) {
                        braid(p, headwear_parts::WINGED_HELM_HAIR_ROOT, hair_color(l));
                    }
                }
                Headwear::Crown { metal, gem } => headwear_parts::crown(p, metal, gem),
                Headwear::Bare => {}
                Headwear::Tricorn { color, trim } => headwear_parts::tricorn(p, color, trim),
                Headwear::WitchHat { color, band } => headwear_parts::witch_hat(p, color, band),
                Headwear::Wreath(color) => headwear_parts::wreath(p, color),
                Headwear::Circlet { color, gem, broad } => {
                    headwear_parts::circlet(p, color, gem, broad)
                }
                Headwear::Veiled { cap, veil } => headwear_parts::veiled(p, cap, veil),
            }
            if let Some((metal, gem)) = outfit.trimmings.earrings {
                for side in [-1.0, 1.0] {
                    let at = Vec3::new(side * 0.117, 0.085, 0.005);
                    p.metal
                        .prism(at, at - Vec3::Y * 0.035, 0.002, 0.002, 5, metal, 0.0);
                    p.metal.blob(
                        at - Vec3::Y * 0.045,
                        Vec3::new(0.010, 0.017, 0.006),
                        1,
                        0.0,
                        metal,
                    );
                    p.satin.blob(
                        at + Vec3::new(0.0, -0.045, 0.005),
                        Vec3::new(0.007, 0.012, 0.005),
                        1,
                        0.0,
                        gem,
                    );
                }
            }
            match style {
                Style::Hero => {}
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
                        let c = hair_color(l);
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
                _ => rogue::head_details(p, l),
            }
        }
    }
}

/// Skirts, a jerkin's peplum and a fitted robe, layered over the hips.
fn skirts(kit: &mut Kit, figure: &Figure, hips: Entity) {
    let &Figure {
        look: ref l,
        body,
        build: b,
        outfit,
        boots,
        shoes,
        boot_shape,
        blocky,
        furry,
        cuirass,
        trousered,
        ref shell_shape,
        ref waist_support,
        ..
    } = figure;
    // Resolve the lower layer once so the tunic emitter and outer fit share
    // its cut, folds, phase and placement.
    let skirt_surface = |skirt: Skirt, layer: u8| {
        let flare = if cuirass {
            0.185
        } else {
            0.208 + skirt.length * 0.045
        } * skirt.fullness
            * (1.0 - 0.025 * f32::from(layer));
        let length = skirt.length;
        let cut = skirt_cut(body, flare, length, trousered, skirt.cutaway);
        let mut surface = ClothSurface::new(
            cut,
            length,
            if length > 0.5 || skirt.fullness > 1.2 {
                12
            } else {
                8
            },
            skirt.split,
            u64::from(l.v.shade) + u64::from(layer) * 31,
        )
        .with_hem(skirt.hem);
        surface.paired = skirt.center_panel.is_some();
        if surface.paired || skirt.waistline.is_some() {
            let seam = (0..=64)
                .map(|k| {
                    let angle = k as f32 * TAU / 64.0;
                    let height = skirt.waist_height(angle);
                    waist_parts::point(body, waist_support, height, angle, 0.006)
                })
                .collect();
            surface.seam = Some(std::sync::Arc::new(seam));
            surface = surface.fit_pelvis(body);
        }
        surface
    };
    let mut skirt_stack = Vec::new();
    if let Some(skirt) = outfit.skirt.filter(|_| !blocky && !furry) {
        skirt_stack.push((0u8, skirt, skirt_surface(skirt, 0)));
        for (slot, layer) in skirt.layers.into_iter().enumerate() {
            if let Some(layer) = layer {
                let index = slot as u8 + 1;
                let under = layer.resolve(skirt);
                skirt_stack.push((index, under, skirt_surface(under, index)));
            }
        }
    }
    // Resolve the stack from the innermost garment outward. Each outer
    // layer clears every lower layer, including through another layer's slit.
    for outer in (0..skirt_stack.len()).rev() {
        for inner in outer + 1..skirt_stack.len() {
            let under = skirt_stack[inner].2.clone();
            let garment = skirt_stack[inner].1;
            let raised_trim =
                garment.trim.is_some() || matches!(garment.pattern, Pattern::Brocade { .. });
            skirt_stack[outer].2 =
                skirt_stack[outer]
                    .2
                    .clone()
                    .fit_over(under, Affine3A::IDENTITY, raised_trim);
        }
    }
    let skirt_layer = skirt_stack.first().map(|(_, _, surface)| surface.clone());
    let skirt_scale = storybook_part(Plan::Biped, J::Hips);
    for (index, skirt, surface) in skirt_stack {
        let name = if index == 0 {
            "skirt".to_owned()
        } else {
            format!("skirt{index}")
        };
        let entity = kit.drape_scaled(
            hips,
            Transform::IDENTITY,
            Drape::Skirt(skirt.length),
            &name,
            skirt_scale,
            |p| {
                let b = if skirt.sheer {
                    &mut p.sheer
                } else {
                    &mut p.matte
                };
                skirt_cloth(b, surface, skirt);
            },
        );
        kit.rig.skirt_layers.push(entity);
    }
    // A jerkin's free region: below the belt its leather is no longer
    // supported by the body and hangs as a short flared peplum over the tunic.
    if let Some(Outer::Shell {
        peplum: Some(peplum),
        ..
    }) = outfit.outer
    {
        let (top, offset) = shell_waist(
            shell_shape.as_ref().expect("peplum has a supporting shell"),
            body,
            0.075,
        );
        let bottom = Vec2::new(0.215 * b * body.hips_width, 0.162 * b);
        // The fitted seam and the free hip/hem have different supports.
        // Starting the hip at the narrow waist would make lower-layer fitting
        // enlarge an already sufficient hem by the same excessive factor.
        let cut = SkirtCut {
            waist: top,
            hip: bottom,
            hip_at: 0.5,
            hem: bottom,
            cutaway: peplum.cutaway,
        };
        let mut surface =
            ClothSurface::new(cut, peplum.length, 6, peplum.split, u64::from(l.v.pattern));
        let support = shell_shape.as_ref().expect("peplum has a supporting shell");
        let scale = storybook_part(Plan::Biped, J::Torso);
        surface.seam = Some(std::sync::Arc::new(
            (0..=64)
                .map(|k| {
                    waist_parts::point(body, support, 0.075, k as f32 * TAU / 64.0, 0.003) * scale
                        - offset
                })
                .collect(),
        ));
        if let Some(under) = skirt_layer {
            surface = surface.fit_over(
                under,
                Affine3A::from_scale_rotation_translation(skirt_scale, Quat::IDENTITY, -offset),
                outfit.skirt.is_some_and(|skirt| skirt.trim.is_some()),
            );
        }
        kit.drape(
            hips,
            Transform::from_translation(offset),
            Drape::Skirt(peplum.length),
            "peplum",
            |p| draped_cloth(&mut p.satin, surface, peplum.color, peplum.trim),
        );
    }
    if let Some(robe) = outfit.robe {
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
            color: robe.trim * 0.8,
        });
        kit.rig.robe = Some(super::robe::spawn(
            kit.commands,
            kit.library,
            kit.meshes,
            hips,
            robe,
            body,
            boot_shape,
            if (boots.is_some() || shoes.is_some()) && !blocky && !furry {
                BootLast {
                    build: b,
                    heeled: boots.is_some_and(|boots| boots.heel)
                        || shoes.is_some_and(|shoes| shoes.heel),
                }
                .contacts()
            } else {
                body.feet
            },
            &sashes,
        ));
    }
}

/// Both arms, from the shoulder to the hand, with their sleeves and armour.
fn arms(kit: &mut Kit, figure: &Figure, torso: Entity) {
    let &Figure {
        look: ref l,
        style,
        body,
        build: b,
        arm_len,
        outfit,
        blocky,
        furry,
        bare_arms,
        sleeve,
        fore_color,
        styled,
        hand_color,
        armor_item,
        ..
    } = figure;
    for (side, arm, fore, hand) in [
        (1.0f32, J::ArmL, J::ForeL, J::HandL),
        (-1.0, J::ArmR, J::ForeR, J::HandR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let shoulder = body.shoulder(side);
        let metal = outfit.arms == Arms::Plate;
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
            if let (false, Some(finish)) =
                (metal, armor_item.and_then(armor_parts::shoulder_finish))
            {
                // Lighter armours cap the shoulder over the sleeve, as on the sheets.
                biped_pauldron(p, side, body, finish);
            }
            if metal {
                biped_pauldron(p, side, body, PauldronFinish::STEEL);
                // Rerebrace lames below the pauldron, each tucked under the last.
                plate_lames(&mut p.metal, &rerebrace_lames(body));
            }
            if let Some(fringe) = outfit.trimmings.shoulders {
                fringe_parts::shoulder(
                    p,
                    fringe,
                    Vec3::new(side * 0.015, 0.025, 0.0),
                    Vec2::new(0.105, 0.095) * b,
                    b,
                );
            }
            if let Some(styled) = styled {
                styled_sleeve_top(p, styled, side, b, arm_len);
            }
            if let Some(color) = outfit.trimmings.armlets {
                accessory_parts::armlet(p, color, b, arm_len);
            }
            if let Some(color) = outfit.trimmings.peaks {
                accessory_parts::peak(p, color, side, b);
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
                let fore_bare = bare_arms
                    && !styled.is_some_and(|styled| {
                        matches!(styled.cut, SleeveCut::Gauntlet | SleeveCut::Draped)
                    });
                biped_forearm(m, b, arm_len, body.profile.muscle, fore_bare, fore_color);
            }
            let cuff = outfit.cuffs;
            if let Some(styled) = styled.filter(|styled| {
                matches!(
                    styled.cut,
                    SleeveCut::Bell | SleeveCut::Gauntlet | SleeveCut::Draped
                )
            }) {
                // A bell flares from the elbow; a gauntlet's drape is sheer.
                let flare = styled.flare.unwrap_or(styled.color);
                let builder = if styled.cut == SleeveCut::Gauntlet {
                    &mut p.sheer
                } else {
                    &mut p.matte
                };
                biped_cuff(
                    builder,
                    b * 1.15,
                    arm_len,
                    body.profile.muscle,
                    true,
                    flare,
                    flare * 0.8,
                );
            } else if let Some(robe) = outfit.robe {
                biped_cuff(m, b, arm_len, body.profile.muscle, true, sleeve, robe.trim);
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
            if let Some(metal) = outfit.trimmings.bracelets {
                p.metal.torus(
                    Vec3::Y * (-0.255 * arm_len),
                    Vec3::Y,
                    0.041 * b,
                    0.004 * b,
                    24,
                    5,
                    metal,
                );
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
                // A woman's free hand is open and relaxed, ready to gesture.
                let open = clawed
                    || body.bust.is_some()
                    || matches!(style, Style::Caster | Style::Leprechaun | Style::Nymph);
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
}

/// Both legs, from the thigh to the foot, with their garments and footwear.
fn legs(kit: &mut Kit, figure: &Figure, hips: Entity) {
    let &Figure {
        look: ref l,
        style,
        body,
        build: b,
        outfit,
        boots,
        shoes,
        boot_shape,
        blocky,
        furry,
        pants,
        barefoot,
        trousered,
        boot,
        ..
    } = figure;
    for (side, leg, shin, foot) in [
        (1.0f32, J::LegL, J::ShinL, J::FootL),
        (-1.0, J::LegR, J::ShinR, J::FootR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, hips, body.leg_root(side), &format!("leg{tag}"), |p| {
            if let Some(color) = outfit.trimmings.garter.filter(|_| side > 0.0 && !blocky) {
                accessory_parts::garter(p, color, b);
            }
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
            } else if outfit.legs == Legs::Plate {
                // Cuisses over the mail: overlapping lames down to the knee cop.
                sculpted_limb(&mut p.matte, body.thigh, pants);
                plate_lames(&mut p.metal, &cuisse_lames(body));
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
            if outfit.legs == Legs::Plate {
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
            if boots.is_some() {
                if let Some(fringe) = outfit.trimmings.boot_tops {
                    fringe_parts::rim(
                        p,
                        fringe,
                        Vec3::Y * boot_top,
                        Vec2::splat(boot_shape.radius_top + 0.008 * b),
                        b,
                    );
                }
            }
            // Bare legs end in the foot itself, not in a shaft round the
            // ankle, unless boots are pulled on over them.
            let bare_leg = matches!(outfit.legs, Legs::Bare(_)) && boots.is_none();
            if !bare_leg {
                p.satin.prism(
                    Vec3::Y * boot_top,
                    Vec3::Y * boot_shape.bottom,
                    boot_shape.radius_top,
                    boot_shape.radius_bottom,
                    8,
                    boot,
                    0.0,
                );
            }
            if let Some(shoes) = shoes {
                // An ankle strap, and for gladiator sandals straps laced
                // up the shin, each round its own girth.
                let low = boot_shape.bottom + 0.012;
                let bands = 1 + (shoes.straps * 6.0).round() as usize;
                for k in 0..bands {
                    let y = low + 0.05 * k as f32;
                    let girth = 0.045 + 0.022 * ((y - low) / 0.27).clamp(0.0, 1.0);
                    p.satin.torus(
                        Vec3::Y * y,
                        Vec3::Y,
                        girth * b + 0.004,
                        0.006,
                        12,
                        3,
                        shoes.color,
                    );
                }
            }
            if barefoot && !bare_leg {
                // Wrappings bind the ankle and calf of a bare leg.
                for y in [-0.3f32, -0.36] {
                    p.satin
                        .torus(Vec3::Y * y, Vec3::Y, 0.062 * b, 0.01, 12, 3, LEATHER * 0.8);
                }
            } else if boots.is_some_and(|boots| boots.cuffed) {
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
            if boots.is_some_and(|boots| boots.buckles) {
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
            if let Some(shoes) = shoes.filter(|_| !furry) {
                p.set_transform(Affine3A::IDENTITY);
                let upper = if shoes.sandal { l.skin } else { shoes.color };
                boot_parts::emit_shoe(
                    p,
                    BootLast {
                        build: b,
                        heeled: shoes.heel,
                    },
                    upper,
                    shoes.color,
                );
            } else if let Some(boots) = boots.filter(|_| !furry) {
                p.set_transform(Affine3A::IDENTITY);
                boot_parts::emit(
                    p,
                    BootLast {
                        build: b,
                        heeled: boots.heel,
                    },
                    boot,
                );
            } else {
                // Bare and furry families retain their anatomical foot recipe.
                for (i, (center, radii)) in body.feet.into_iter().enumerate() {
                    p.satin.blob(
                        center - Vec3::Y * 0.4,
                        radii,
                        (2 - i) as u64,
                        0.0,
                        if i == 2 { boot * 0.55 } else { boot },
                    );
                }
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
    rogue::biped_features(kit, l);
}

/// Long loose hair hung from the crown so it can swing on its own.
fn loose_hair(kit: &mut Kit, figure: &Figure, head: Entity) {
    let &Figure {
        look: ref l,
        body,
        fem,
        outfit,
        trim_neck,
        ref shell_shape,
        ref native_plate,
        ref equipped_plate,
        ..
    } = figure;
    // Long loose hair hangs from the crown on its own and swings as she
    // moves, cut in the head's space exactly as it would sit on the head.
    if loose_fall(l) {
        let size = if fem { 1.1 } else { 1.14 };
        let face = Affine3A::from_translation(Vec3::Y * 0.08)
            * Affine3A::from_scale(Vec3::splat(size))
            * Affine3A::from_translation(Vec3::Y * -0.08);
        let proportion = storybook_part(Plan::Biped, J::Head);
        let strands = long_hair_strands(hair_seed(l, 0));
        let models: Vec<_> = strands
            .iter()
            .enumerate()
            .map(|(index, strand)| {
                kit.part(&format!("hair/{index}"), |p| {
                    p.set_transform(Affine3A::from_translation(-CROWN) * face);
                    hanging_lock(p, l, strand);
                })
            })
            .collect();
        let mut support = torso_shape(body, None, trim_neck);
        if let (Some(bust), Some(share)) = (body.bust, outfit.bodice.support.share()) {
            bust_underbridge(&mut support, &bust, share, 0.0, 0.0);
        }
        for layer in [shell_shape, native_plate, equipped_plate]
            .into_iter()
            .flatten()
        {
            support.union(layer.clone(), 0.0);
        }
        let hair_frame = Affine3A::from_translation(-CROWN) * face;
        let curves: Vec<super::deformation::ControlCurve> = strands
            .into_iter()
            .map(|strand| {
                let last = strand.path.len() - 1;
                let points = strand
                    .path
                    .into_iter()
                    .enumerate()
                    .map(|(i, at)| {
                        let t = i as f32 / last as f32;
                        (
                            hair_frame.transform_point3(at),
                            (strand.root + (strand.tip - strand.root) * t * t) * size,
                        )
                    })
                    .collect();
                super::deformation::ControlCurve {
                    points,
                    attached: strand.attached,
                    support_side: strand.support_side,
                }
            })
            .collect();
        let contacts = super::visual_contact::posed(&kit.rig.visual_contacts, |joint| {
            Some(Affine3A::from_translation(kit.rig.rest[joint as usize]))
        });
        let hair = super::hair_contact::spawn(
            kit.commands,
            kit.meshes,
            head,
            Transform::from_translation(CROWN * proportion),
            proportion,
            &models,
            support,
            Affine3A::from_translation(body.head),
            &curves,
            contacts,
        );
        kit.rig.drapes.push((hair.entity, Drape::Hair));
        kit.rig.hair = Some(hair);
    }
}

/// A worn cloak, fitted to the wearer's shoulders and torso layers.
fn cloak(kit: &mut Kit, figure: &Figure, torso: Entity) {
    let &Figure {
        body,
        outfit,
        ref equipped_plate,
        ..
    } = figure;
    // The garment owns its palette and cut; the wearer supplies its fit.
    if let Some(cloak) = outfit.cloak {
        let mut recipe =
            super::cloak::Recipe::new(body, cloak.exterior, cloak.lining, cloak.ragged);
        recipe.length = cloak.length;
        if let Some(armor) = &equipped_plate {
            recipe.reserve_body_layer(armor);
        }
        if outfit.baldric.is_some() {
            recipe.reserve_back_layer(BALDRIC_SURFACE_OFFSET);
        }
        kit.cloak(torso, recipe);
    }
}
