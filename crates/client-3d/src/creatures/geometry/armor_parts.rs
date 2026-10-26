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

pub(super) fn emit(p: &mut Parts, plate: &crate::sdf::Sculpt, body: HumanoidBody) {
    let b = body.build.max(1.0);
    let opening = opening(body).map(|shape| {
        let mut cut = crate::sdf::Sculpt::default();
        cut.add(shape, 0.0);
        cut
    });
    plate.mesh(
        &mut p.metal,
        Vec3::new(-0.42, 0.035, -0.32) * b,
        Vec3::new(0.42, 0.51, 0.35) * b,
        0.008,
        &|at, normal| {
            let edge = opening
                .as_ref()
                .is_some_and(|cut| cut.distance(at).abs() < 0.008);
            if edge {
                GILT
            } else {
                STEEL * (0.92 + 0.08 * normal.z.abs())
            }
        },
    );
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

#[cfg(test)]
mod tests {
    use super::*;

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
