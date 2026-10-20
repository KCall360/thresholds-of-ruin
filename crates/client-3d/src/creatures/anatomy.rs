//! Resolved body dimensions shared by generation, garment fitting and motion.
//! Existing family recipes resolve linked build silhouettes here. Storybook
//! mesh scales enlarge masses separately from the body's joint landmarks.
use super::{Look, Plan, Style, Vec3, J};

/// Where the head sits on the torso: just above the shoulder line.
pub(super) const NECK_SEAT: f32 = 0.535;

pub(super) const HIP_SCALE: Vec3 = Vec3::new(1.10, 1.0, 1.08);
pub(super) const LIMB_SCALE: Vec3 = Vec3::new(1.12, 1.0, 1.12);
pub(super) const FOOT_SCALE: Vec3 = Vec3::new(1.22, 1.0, 1.22);

/// Correlated silhouette changes around the unchanged average build. These
/// ratios describe one existing build choice, not independent random knobs.
#[derive(Clone, Copy)]
pub(super) struct BuildProfile {
    pub shoulders: f32,
    pub waist: f32,
    pub hips: f32,
    pub muscle: f32,
}

impl BuildProfile {
    fn resolve(style: Style, build: u8) -> Self {
        // Stone and fur have deliberately separate mass/construction recipes.
        let i = if matches!(style, Style::Guardian | Style::Yeti) {
            1
        } else {
            build as usize
        };
        Self {
            shoulders: [0.94, 1.0, 1.08][i],
            waist: [0.92, 1.0, 1.04][i],
            hips: [0.96, 1.0, 1.07][i],
            muscle: [0.94, 1.0, 1.08][i],
        }
    }
}

pub(super) fn storybook_part(plan: Plan, joint: J) -> Vec3 {
    match joint {
        J::Head => match plan {
            Plan::Bird => Vec3::new(1.25, 1.16, 1.25),
            Plan::Quadruped => Vec3::new(1.18, 1.12, 1.18),
            _ => Vec3::new(1.18, 1.15, 1.16),
        },
        J::Torso | J::Hips => HIP_SCALE,
        J::Body => Vec3::new(1.12, 1.0, 1.10),
        J::ArmL | J::ArmR | J::ForeL | J::ForeR => LIMB_SCALE,
        J::LegL | J::LegR | J::ShinL | J::ShinR => LIMB_SCALE,
        J::FootL | J::FootR => FOOT_SCALE,
        J::FL | J::FR | J::BL | J::BR | J::FLs | J::FRs | J::BLs | J::BRs => {
            Vec3::new(1.16, 1.0, 1.16)
        }
        J::Neck => Vec3::new(1.08, 1.0, 1.08),
        _ => Vec3::ONE,
    }
}

/// The taper and muscle belly consumed by the sculpt and its fitting proxy.
#[derive(Clone, Copy)]
pub(super) struct LimbShape {
    pub length: f32,
    pub radius_top: f32,
    pub radius_bottom: f32,
    pub belly: Vec3,
    pub belly_center: Vec3,
}

impl LimbShape {
    pub fn new(length: f32, radius_top: f32, radius_bottom: f32, belly: f32, forward: f32) -> Self {
        Self {
            length,
            radius_top,
            radius_bottom,
            belly: Vec3::new(belly, length * 0.32, belly * 1.05),
            belly_center: Vec3::new(0.0, -length * 0.35, forward),
        }
    }
}

/// Standard boot dimensions are a fitted outfit choice, separate from anatomy.
#[derive(Clone, Copy)]
pub(super) struct BootShape {
    pub top: f32,
    pub bottom: f32,
    pub radius_top: f32,
    pub radius_bottom: f32,
    pub cuff_radius: f32,
    pub cuff_thickness: f32,
    // Existing point-sampling envelope, distinct from the authored shaft.
    pub sample_radius_top: f32,
    pub sample_radius_bottom: f32,
}

impl BootShape {
    pub fn fitted(build: f32, top: f32) -> Self {
        Self {
            top,
            bottom: -0.42,
            radius_top: 0.073 * build,
            radius_bottom: 0.064 * build,
            cuff_radius: 0.075 * build,
            cuff_thickness: 0.016,
            sample_radius_top: 0.078 * build,
            sample_radius_bottom: 0.067 * build,
        }
    }
}

/// Resolved once per biped. Outfit palettes, coverage and motion choices stay
/// in their recipes; changing those does not require reconstructing anatomy.
#[derive(Clone, Copy)]
pub(super) struct HumanoidBody {
    pub woman: bool,
    pub build: f32,
    pub profile: BuildProfile,
    pub shoulders: f32,
    pub waist: f32,
    pub hips_width: f32,
    pub arm_length: f32,
    pub hips: Vec3,
    pub torso: Vec3,
    pub head: Vec3,
    pub hand: Vec3,
    pub thigh: LimbShape,
    pub shin: LimbShape,
    /// Existing rounded terminal-foot primitives, in ankle-local coordinates.
    /// Stone feet and furry claw additions keep their family recipes; this
    /// does not define arbitrary footwear or armor compatibility.
    pub feet: [(Vec3, Vec3); 3],
}

impl HumanoidBody {
    pub fn resolve(look: &Look) -> Self {
        let build = match look.style {
            Style::Guardian => 1.4,
            Style::Yeti => 1.45,
            Style::Hobgoblin => 1.08,
            Style::Zombie => 0.92,
            Style::Vampire => 0.9,
            Style::Caster | Style::Scout => 0.92,
            Style::Orc => 1.16,
            Style::Troll => 1.26,
            Style::Leprechaun | Style::Nymph | Style::Medusa => 0.95,
            Style::Phantom | Style::Wraith => 0.9,
            _ => 1.0,
        } * [0.94, 1.0, 1.07][look.v.build as usize];
        let (shoulders, waist, hips_width): (f32, f32, f32) = if look.v.woman {
            (0.9, 0.75, 1.22)
        } else {
            (1.04, 0.94, 1.0)
        };
        let profile = BuildProfile::resolve(look.style, look.v.build);
        // Hip spread and muscle volume share the profile without multiplying
        // both width gains into the same thigh belly. Preserve the sex shape.
        let thigh_belly = 0.092 * build * hips_width.max(1.0) * profile.muscle;
        let (shoulders, waist, hips_width) = (
            shoulders * profile.shoulders,
            waist * profile.waist,
            hips_width * profile.hips,
        );
        let arm_length = match look.style {
            Style::Yeti => 1.25,
            // Trolls' long arms hang to their knees.
            Style::Troll => 1.18,
            _ => 1.0,
        };
        Self {
            woman: look.v.woman,
            build,
            profile,
            shoulders,
            waist,
            hips_width,
            arm_length,
            hips: Vec3::Y * 0.95,
            torso: Vec3::Y * 0.06,
            // The head sits close on the shoulders on a short neck, as on
            // the concept sheets, not raised on a stalk.
            head: Vec3::Y * NECK_SEAT,
            hand: Vec3::Y
                * if look.style == Style::Guardian {
                    -0.38
                } else {
                    -0.31 * arm_length
                },
            // Hers flow full out of the hip and taper to a finer ankle under
            // a shapely calf, as on the concept sheets.
            thigh: LimbShape::new(
                0.44,
                0.088 * build * if look.v.woman { 1.1 } else { 1.0 },
                0.06 * build,
                thigh_belly,
                0.008,
            ),
            shin: if look.v.woman {
                LimbShape::new(0.4, 0.062 * build, 0.042 * build, 0.07 * build, -0.016)
            } else {
                LimbShape::new(0.4, 0.062 * build, 0.048 * build, 0.068 * build, -0.014)
            },
            feet: [
                (
                    Vec3::new(0.0, -0.025, 0.05 * build),
                    Vec3::new(0.062, 0.042, 0.11) * build,
                ),
                (
                    Vec3::new(0.0, -0.04, 0.13 * build),
                    Vec3::new(0.058, 0.03, 0.055) * build,
                ),
                (
                    Vec3::new(0.0, -0.068, 0.06 * build),
                    Vec3::new(0.064, 0.008, 0.13) * build,
                ),
            ],
        }
    }

    pub fn pelvis_radii(self) -> Vec3 {
        // Her seat is rounder, front to back.
        let depth = if self.woman { 0.14 } else { 0.13 };
        Vec3::new(0.17 * self.hips_width, 0.12, depth) * self.build
    }

    pub fn leg_root(self, side: f32) -> Vec3 {
        Vec3::new(side * 0.095 * self.build * self.hips_width, -0.02, 0.0)
    }

    pub fn shoulder(self, side: f32) -> Vec3 {
        Vec3::new(side * 0.25 * self.build * self.shoulders, 0.5, 0.0)
    }

    pub fn elbow(self) -> Vec3 {
        Vec3::Y * -0.3 * self.arm_length
    }

    pub fn knee(self) -> Vec3 {
        Vec3::Y * -self.thigh.length
    }

    pub fn ankle(self) -> Vec3 {
        Vec3::Y * -self.shin.length
    }
}
