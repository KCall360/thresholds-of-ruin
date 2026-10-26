//! Independent torso armor fitted over the selected garment layers.
use super::*;

pub(super) const TOP: f32 = 0.485;
pub(super) const BOTTOM: f32 = 0.055;

/// The female front uses the same anatomy-resolved opening as fitted bodices.
/// Keep the cutter available to coverage checks and edge decoration as well.
pub(super) fn opening(body: HumanoidBody) -> Option<crate::sdf::Shape> {
    body.bust?;
    NecklineCut::resolve(
        Neckline::Plunge {
            depth: 0.65,
            width: 0.16,
            wrap: false,
        },
        body,
    )
    .map(|cut| cut.opening(body, TOP, 0.012))
}

pub(super) fn shell(
    body: HumanoidBody,
    under: Option<&crate::sdf::Sculpt>,
    trim_neck: bool,
    decoration_depth: f32,
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape::Beyond;
    // A contoured front follows the paired chest supports; the male variant
    // retains the broad pectoral plate. Both still enclose their underlayers.
    let material = if body.bust.is_some() {
        TorsoMaterial::Leather
    } else {
        TorsoMaterial::Plate
    };
    let mut plate = torso_shape(body, Some(material), trim_neck);
    if let Some(under) = under {
        plate.union(under.clone(), 0.0);
    }
    // Clearance includes the existing garment's sewn surface relief. This
    // is shared with accessory fitting and cloak contact, not a second proxy.
    plate
        .inflate(0.020 + decoration_depth)
        .carve(Beyond(Vec3::Y, TOP), 0.003)
        .carve(Beyond(Vec3::NEG_Y, -BOTTOM), 0.003);
    if let Some(cut) = opening(body) {
        plate.carve(cut, 0.003);
    }
    plate
}

/// Boiled leather and the darker leather that backs rings and splints.
const BOILED: Vec3 = Vec3::new(0.43, 0.24, 0.12);
const BACKING: Vec3 = Vec3::new(0.29, 0.16, 0.08);
const STITCH: Vec3 = Vec3::new(0.88, 0.8, 0.62);

/// A detail site on the armour's surface: where it sits, its outward normal
/// and a tangent running around the body.
#[derive(Clone, Copy, Debug)]
pub(super) struct Site {
    pub at: Vec3,
    pub normal: Vec3,
    pub around: Vec3,
}

/// Points around the shell at one height, at equal arc spacing. Staggered
/// rows are offset half a step, as studs, rings and scales are in the style
/// guide. Sites inside the female opening are left out.
pub(super) fn ring_sites(
    plate: &crate::sdf::Sculpt,
    opening: Option<&crate::sdf::Sculpt>,
    y: f32,
    spacing: f32,
    stagger: bool,
) -> Vec<Site> {
    const SAMPLES: usize = 96;
    let around: Vec<Vec3> = (0..=SAMPLES)
        .map(|i| {
            let angle = i as f32 / SAMPLES as f32 * std::f32::consts::TAU;
            let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
            plate.onto(Vec3::Y * y + outward * 0.45, -outward)
        })
        .collect();
    let lengths: Vec<f32> = around.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f32 = lengths.iter().sum();
    let count = (total / spacing).floor().max(1.0) as usize;
    let mut sites = Vec::with_capacity(count);
    let (mut segment, mut walked) = (0, 0.0);
    for k in 0..count {
        let target = (k as f32 + if stagger { 0.5 } else { 0.0 }) / count as f32 * total;
        while segment + 1 < lengths.len() && walked + lengths[segment] < target {
            walked += lengths[segment];
            segment += 1;
        }
        let t = ((target - walked) / lengths[segment].max(1e-6)).clamp(0.0, 1.0);
        let at = around[segment].lerp(around[segment + 1], t);
        if opening.is_some_and(|cut| cut.distance(at) < 0.014) {
            continue;
        }
        let horizontal = Vec3::new(at.x, 0.0, at.z).normalize_or(Vec3::Z);
        let normal = sculpt_surface_normal(plate, at, horizontal);
        let around = Vec3::Y.cross(normal).normalize_or(Vec3::X);
        sites.push(Site { at, normal, around });
    }
    sites
}

/// Rows of staggered sites between the armour's hems, top row first.
fn lattice(
    plate: &crate::sdf::Sculpt,
    opening: Option<&crate::sdf::Sculpt>,
    low: f32,
    high: f32,
    pitch: f32,
    spacing: f32,
) -> Vec<(usize, Site)> {
    let rows = ((high - low) / pitch).floor().max(1.0) as usize;
    (0..rows)
        .flat_map(|row| {
            let y = high - (row as f32 + 0.5) * pitch;
            ring_sites(plate, opening, y, spacing, row % 2 == 1)
                .into_iter()
                .map(move |site| (row, site))
        })
        .collect()
}

fn rotation(site: &Site) -> Quat {
    Quat::from_mat3(&Mat3::from_cols(
        site.around,
        site.normal.cross(site.around),
        site.normal,
    ))
}

pub(super) fn emit(
    p: &mut Parts,
    plate: &crate::sdf::Sculpt,
    body: HumanoidBody,
    item: super::super::outfit::BodyItem,
) {
    use super::super::outfit::BodyItem as Item;
    let b = body.build.max(1.0);
    let opening = opening(body).map(|shape| {
        let mut cut = crate::sdf::Sculpt::default();
        cut.add(shape, 0.0);
        cut
    });
    let opening = opening.as_ref();
    let (low, high) = (BOTTOM + 0.018, TOP - 0.022);
    let edge = |at: Vec3| {
        opening.is_some_and(|cut| cut.distance(at).abs() < 0.008)
            || at.y < BOTTOM + 0.008
            || at.y > TOP - 0.008
    };
    // Edges are bound in darker leather; a dashed stitch runs down the centre
    // seam and along the lower hem, thin enough not to fray the outline.
    let stitched = |at: Vec3, leather: Vec3| {
        let seam = at.x.abs() < 0.004;
        let hem = (at.y - (BOTTOM + 0.016)).abs() < 0.0025;
        if edge(at) {
            leather * 0.62
        } else if (seam || hem) && (at.y * 70.0 + at.x * 70.0).rem_euclid(1.0) < 0.5 {
            STITCH
        } else if seam {
            leather * 0.78
        } else {
            leather
        }
    };
    let (min, max) = (
        Vec3::new(-0.42, 0.035, -0.32) * b,
        Vec3::new(0.42, 0.51, 0.35) * b,
    );
    match item {
        Item::Leather | Item::StuddedLeather => {
            plate.mesh(&mut p.matte, min, max, 0.008, &|at, _| stitched(at, BOILED));
        }
        Item::RingMail | Item::SplintMail => {
            plate.mesh(&mut p.matte, min, max, 0.008, &|at, _| {
                stitched(at, BACKING)
            });
        }
        Item::ChainMail => {
            plate.mesh(&mut p.metal, min, max, 0.008, &|at, _| {
                if edge(at) {
                    BACKING
                } else {
                    // Interlocking links as staggered rows of light and shadow,
                    // coarse enough for the sculpt's vertex spacing to carry.
                    let around = at.x.atan2(at.z) * 0.18;
                    let row = (at.y / 0.017).floor();
                    let column = (around / 0.017 + row * 0.5).floor();
                    let lit = (row + column).rem_euclid(2.0) < 1.0;
                    IRON * if lit { 1.45 } else { 0.85 }
                }
            });
        }
        Item::ScaleMail | Item::BandedMail | Item::SteelBreastplate | Item::PlateMail => {
            let polished = matches!(item, Item::SteelBreastplate | Item::PlateMail);
            plate.mesh(&mut p.metal, min, max, 0.008, &|at, normal| {
                if edge(at) {
                    GILT
                } else if polished {
                    STEEL * (0.92 + 0.08 * normal.z.abs())
                } else {
                    // Backing between scales and bands is leather and mail.
                    IRON * 0.9
                }
            });
        }
    }
    match item {
        Item::Leather => side_straps(p, plate, opening),
        Item::StuddedLeather => {
            for (_, site) in lattice(plate, opening, low, high, 0.04 * b, 0.042 * b) {
                p.metal.prism(
                    site.at - site.normal * 0.002,
                    site.at + site.normal * 0.007,
                    0.0065,
                    0.0045,
                    8,
                    GILT,
                    0.0,
                );
            }
            side_straps(p, plate, opening);
        }
        Item::RingMail => {
            for (_, site) in lattice(plate, opening, low, high, 0.046 * b, 0.048 * b) {
                p.metal.torus(
                    site.at + site.normal * 0.005,
                    site.normal,
                    0.014,
                    0.0038,
                    10,
                    4,
                    IRON * 1.25,
                );
                // Each ring is sewn on by a leather tab across its top.
                p.matte.cuboid(
                    site.at + site.normal * 0.006 + Vec3::Y * 0.012,
                    Vec3::new(0.0045, 0.006, 0.0035),
                    rotation(&site),
                    BACKING * 1.2,
                );
            }
        }
        Item::ScaleMail => {
            // Upper rows lie over the rows below, each tip pointing down and
            // lifting slightly off the backing.
            for (row, site) in lattice(plate, opening, low, high + 0.012, 0.036 * b, 0.046 * b) {
                let down = (Vec3::NEG_Y * 0.94 + site.normal * 0.34).normalize();
                p.metal.extrude(
                    &[
                        Vec2::new(-0.025, 0.016),
                        Vec2::new(0.025, 0.016),
                        Vec2::new(0.023, -0.016),
                        Vec2::new(0.0, -0.040),
                        Vec2::new(-0.023, -0.016),
                    ],
                    site.at + site.normal * (0.004 + 0.0012 * row as f32),
                    site.around,
                    down,
                    0.003,
                    STEEL * (0.86 + 0.08 * ((row % 3) as f32)),
                );
            }
        }
        Item::SplintMail => {
            // Vertical steel splints riveted at each end, gathered by a band.
            // A splint crossing the female opening is cut where it meets the
            // neckline, so splints still flank the V as on the sheet.
            const STEPS: usize = 13;
            for site in ring_sites(plate, None, (low + high) * 0.5, 0.044 * b, false) {
                let angle = site.at.x.atan2(site.at.z);
                let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
                let path: Vec<Vec3> = (0..STEPS)
                    .map(|k| {
                        let y = low + (high - low) * k as f32 / (STEPS - 1) as f32;
                        plate.onto(Vec3::Y * y + outward * 0.45, -outward) + outward * 0.006
                    })
                    .collect();
                let clear = |at: &Vec3| !opening.is_some_and(|cut| cut.distance(*at) < 0.016);
                for run in path.split(|at| !clear(at)).filter(|run| run.len() >= 3) {
                    p.metal.tube_framed(
                        run,
                        &vec![Vec2::new(0.015, 0.003); run.len()],
                        4,
                        &|_, _, _| STEEL,
                        Some(site.around),
                    );
                    for end in [run[0], run[run.len() - 1]] {
                        p.metal.prism(
                            end,
                            end + outward * 0.006,
                            0.004,
                            0.003,
                            6,
                            STEEL * 1.1,
                            0.0,
                        );
                    }
                }
            }
            fitted_band(&mut p.matte, plate, 0.11, 0.135, 0.012, LEATHER);
        }
        Item::BandedMail => {
            // Overlapping horizontal bands below the neckline, brass-riveted.
            let ceiling = opening_floor(opening).map_or(high, |floor| floor.min(high));
            let mut y = low;
            let mut k = 0;
            while y + 0.03 <= ceiling {
                fitted_band(
                    &mut p.metal,
                    plate,
                    y,
                    y + 0.034,
                    0.006 + 0.0015 * k as f32,
                    STEEL * (1.0 - 0.025 * (k % 2) as f32),
                );
                for site in ring_sites(plate, None, y + 0.017, 0.11 * b, k % 2 == 1) {
                    p.metal.prism(
                        site.at + site.normal * 0.004,
                        site.at + site.normal * 0.011,
                        0.0045,
                        0.0035,
                        6,
                        GILT,
                        0.0,
                    );
                }
                y += 0.03;
                k += 1;
            }
        }
        Item::ChainMail => {
            fitted_band(
                &mut p.matte,
                plate,
                BOTTOM + 0.004,
                BOTTOM + 0.016,
                0.004,
                LEATHER,
            );
        }
        Item::SteelBreastplate | Item::PlateMail => {}
    }
    if matches!(
        item,
        Item::SteelBreastplate | Item::PlateMail | Item::BandedMail | Item::ScaleMail
    ) {
        fitted_band(
            &mut p.metal,
            plate,
            BOTTOM + 0.006,
            BOTTOM + 0.013,
            0.003,
            GILT,
        );
        // A circumferential band would bridge the open neckline with triangles.
        if opening.is_none() {
            fitted_band(&mut p.metal, plate, TOP - 0.013, TOP - 0.006, 0.003, GILT);
        }
    }
}

/// Shoulder caps for armours that leave the sleeves free; plated arms bring
/// their own steel pauldrons, and mail shirts have none.
pub(super) fn shoulder_finish(item: super::super::outfit::BodyItem) -> Option<PauldronFinish> {
    use super::super::outfit::BodyItem as Item;
    let leather = |rim| PauldronFinish {
        leather: true,
        face: BOILED,
        rim,
    };
    match item {
        Item::Leather => Some(leather(STITCH)),
        Item::StuddedLeather => Some(leather(GILT)),
        Item::RingMail => Some(PauldronFinish {
            face: BACKING,
            ..leather(IRON * 1.25)
        }),
        Item::ScaleMail => Some(PauldronFinish {
            leather: false,
            face: STEEL * 0.9,
            rim: BACKING,
        }),
        _ => None,
    }
}

/// The lowest height the female opening reaches in front of the sternum.
pub(super) fn opening_floor(opening: Option<&crate::sdf::Sculpt>) -> Option<f32> {
    let cut = opening?;
    (0..80)
        .map(|k| TOP - k as f32 * 0.005)
        .filter(|&y| (0..20).any(|d| cut.distance(Vec3::new(0.0, y, 0.05 + d as f32 * 0.01)) < 0.0))
        .reduce(f32::min)
}

/// Two buckled straps closing each flexible side, as on the leather sheets.
fn side_straps(p: &mut Parts, plate: &crate::sdf::Sculpt, opening: Option<&crate::sdf::Sculpt>) {
    for y in [0.2, 0.28] {
        for side in [-1.0f32, 1.0] {
            let outward = Vec3::new(side, 0.0, 0.25).normalize();
            let at = plate.onto(Vec3::Y * y + outward * 0.45, -outward);
            if opening.is_some_and(|cut| cut.distance(at) < 0.014) {
                continue;
            }
            let normal = sculpt_surface_normal(plate, at, outward);
            let around = Vec3::Y.cross(normal).normalize_or(Vec3::X);
            let site = Site { at, normal, around };
            p.matte.cuboid(
                at + normal * 0.004,
                Vec3::new(0.034, 0.008, 0.003),
                rotation(&site),
                BACKING,
            );
            p.metal.cuboid(
                at + normal * 0.008 + around * side * 0.012,
                Vec3::new(0.007, 0.010, 0.002),
                rotation(&site),
                GILT,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn woman(build: u8) -> HumanoidBody {
        HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                build,
                bust: 3,
                ..default()
            }),
        ))
    }

    #[test]
    fn surface_details_keep_out_of_the_opening_and_sit_on_the_armour() {
        for build in [0, 2] {
            let body = woman(build);
            let plate = shell(body, None, true, 0.0);
            let mut cut = crate::sdf::Sculpt::default();
            cut.add(opening(body).expect("female opening"), 0.0);
            let sites = lattice(&plate, Some(&cut), BOTTOM + 0.018, TOP - 0.022, 0.04, 0.042);
            assert!(
                sites.len() > 60,
                "a full lattice of details: {}",
                sites.len()
            );
            for (_, site) in &sites {
                assert!(cut.distance(site.at) >= 0.014, "detail inside the opening");
                assert!(
                    plate.distance(site.at).abs() < 0.002,
                    "detail floats off the shell"
                );
                assert!(site.normal.dot(site.around).abs() < 1e-3);
            }
            // Staggered rows really are offset, rather than stacking in columns.
            let row = |r| {
                sites
                    .iter()
                    .filter(move |(k, _)| *k == r)
                    .map(|(_, s)| s.at.x)
            };
            let first = row(2).fold(f32::MAX, |a, x| a.min(x.abs()));
            let second = row(3).fold(f32::MAX, |a, x| a.min(x.abs()));
            assert!(
                (first - second).abs() > 0.004,
                "rows {first} and {second} align"
            );
        }
    }

    #[test]
    fn bands_stop_below_the_female_opening() {
        let body = woman(1);
        let mut cut = crate::sdf::Sculpt::default();
        cut.add(opening(body).expect("female opening"), 0.0);
        let floor = opening_floor(Some(&cut)).expect("the opening reaches the sternum");
        assert!(floor > BOTTOM && floor < TOP, "{floor}");
        assert_eq!(opening_floor(None), None);
    }

    #[test]
    fn female_plate_opens_the_upper_sternum_and_keeps_bust_landmarks_covered() {
        for model in [CreatureModel::Human, CreatureModel::Orc] {
            for build in [0, 2] {
                for volume in [0, 4] {
                    let look = Look::with_choice(
                        model,
                        7,
                        Some(Variant {
                            woman: true,
                            build,
                            bust: volume,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let plate = shell(body, None, true, 0.0);
                    let skin = torso_shape(body, None, true);
                    let exposed = torso_surface_hit(&skin, 0.0, 0.455, true).unwrap();
                    assert!(
                        plate.distance(exposed + Vec3::Z * 0.025) > 0.005,
                        "{model:?} build={build} volume={volume}: plate closes the female neckline"
                    );
                    let apex = body.bust.unwrap().apex(1.0);
                    let covered = torso_surface_hit(&plate, apex.x, apex.y, true).unwrap();
                    assert!(
                        covered.z > apex.z + 0.012,
                        "{model:?}: breastplate must still cover the bust apex"
                    );
                }
            }
        }
    }
}
