//! Organic arm surfaces shared by upright bipeds and riders on other body plans.
use super::{closed_surface_step, cloth_over, sleeve_parts, SLEEVE_EASE};
use crate::props::Builder;
use bevy::prelude::Vec3;

fn upper_arm_step(build: f32) -> f32 {
    // Resolve the clavicle bridge and elbow folds before local refinement.
    closed_surface_step(0.02 * build)
}

/// The deltoid, biceps and triceps overlap the clavicle at the arm root. No
/// static shoulder ball is left on the torso when the arm rotates away.
pub(super) fn organic_upper_arm(
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
    arm.mesh_projected(
        b,
        Vec3::new(-0.13 * build, -0.3 * length - 0.06 * build, -0.09 * build),
        Vec3::new(0.13 * build, 0.075 * build.max(1.0), 0.095 * build),
        upper_arm_step(build),
        &|at, _| color * if bare { 1.0 } else { Sculpt::weave(at) },
    );
}

/// A sleeve over the upper arm: eased off the deltoid and biceps, hanging a
/// little fuller beneath, gathered in the crook of the elbow so the bent
/// forearm never shows a gap between the two pieces of cloth.
pub(super) fn organic_upper_sleeve(
    b: &mut Builder,
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    color: Vec3,
) {
    use crate::sdf::Sculpt;
    let sleeve = upper_sleeve_form(side, build, length, muscle);
    sleeve.mesh_projected(
        b,
        Vec3::new(-0.14 * build, -0.3 * length - 0.07 * build, -0.1 * build),
        Vec3::new(0.14 * build, 0.085 * build.max(1.0), 0.105 * build),
        upper_arm_step(build),
        &|at, _| color * Sculpt::weave(at),
    );
}

pub(in crate::creatures) fn upper_sleeve_form(
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
) -> crate::sdf::Sculpt {
    // Ease around the arm, with small compression folds only in its crook.
    let arm = upper_arm_form(side, build, length, 1.0 + (muscle - 1.0) * 0.5, 1.0);
    let mut sleeve = sleeve_parts::gathers(
        cloth_over(arm, SLEEVE_EASE * build, &[]),
        build,
        length,
        &[-0.245, -0.28],
    );
    // Fitting and generation share the complete garment, including its underside.
    sleeve.add(
        crate::sdf::Shape::Ellipsoid(
            Vec3::new(side * 0.012 * build, -0.13 * length, -0.012 * build),
            Vec3::new(0.05 * build, 0.08 * length, 0.05 * build),
        ),
        0.03 * build,
    );
    sleeve
}

pub(in crate::creatures) fn upper_arm_form(
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    fullness: f32,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let mut arm = Sculpt::default();
    // The root overlaps the clavicle's end and the deltoid caps it, both
    // rounding off level with the shoulder line rather than rising over it.
    arm.add(
        Capsule(
            // A horizontal clavicle bridge puts its outer cap around the
            // pivot while keeping the crown below two centimetres per build.
            // This structural socket is shared by bare arms and sleeves.
            Vec3::new(-side * 0.082 * build, -0.04 * build, -0.005),
            Vec3::new(0.0, -0.04 * build, 0.0),
            0.03 * build,
            0.0595 * build,
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
        // These caps already overlap; a wide blend would grow a raised pad.
        0.001 * build,
    )
    .add(
        Ellipsoid(
            Vec3::new(-side * 0.008 * build, -0.08 * length, -0.003),
            Vec3::new(
                0.066 * build * fullness * muscle,
                0.082 * length,
                0.064 * build * muscle,
            ),
        ),
        0.012 * build,
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
pub(super) fn organic_forearm(
    b: &mut Builder,
    build: f32,
    length: f32,
    muscle: f32,
    bare: bool,
    color: Vec3,
) {
    use crate::sdf::Sculpt;
    let arm = if bare {
        forearm_form(build, length, muscle)
    } else {
        forearm_sleeve_form(build, length, muscle)
    };
    arm.mesh_projected(
        b,
        Vec3::new(-0.085 * build, -0.3 * length, -0.085 * build),
        Vec3::new(0.085 * build, 0.065 * build, 0.085 * build),
        closed_surface_step(0.03 * build),
        &|at, _| color * if bare { 1.0 } else { Sculpt::weave(at) },
    );
}

/// A forearm sleeve eases over the extensor mass and bunches just below
/// the elbow, where it tucks under the upper sleeve's folds.
pub(in crate::creatures) fn forearm_sleeve_form(
    build: f32,
    length: f32,
    muscle: f32,
) -> crate::sdf::Sculpt {
    sleeve_parts::gathers(
        cloth_over(
            forearm_form(build, length, 1.0 + (muscle - 1.0) * 0.5),
            SLEEVE_EASE * build * 0.85,
            &[],
        ),
        build,
        length,
        &[-0.025],
    )
}

pub(in crate::creatures) fn forearm_form(
    build: f32,
    length: f32,
    muscle: f32,
) -> crate::sdf::Sculpt {
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
