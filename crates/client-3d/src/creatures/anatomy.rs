//! Resolved body dimensions shared by generation, garment fitting and motion.
//! Existing family recipes resolve linked build silhouettes here. Storybook
//! mesh scales enlarge masses separately from the body's joint landmarks.
use super::{Look, Plan, Style, Vec3, J};

/// The horned reptile recipes share rendered skull and articulated jaw dimensions.
/// Child pivots do not inherit the Storybook scale applied to a parent's mesh.
pub(super) struct ReptileHead {
    pub scale: Vec3,
    pub muzzle: f32,
}

impl ReptileHead {
    pub fn resolve(style: Style) -> Self {
        Self {
            scale: Vec3::splat(if style == Style::Dragon { 1.4 } else { 1.0 }),
            muzzle: if style == Style::Jabberwock {
                1.05
            } else {
                1.0
            },
        }
    }

    pub fn rendered_scale(&self) -> Vec3 {
        self.scale * storybook_part(Plan::Quadruped, J::Head)
    }

    pub fn hinge(&self) -> Vec3 {
        Vec3::new(0.0, -0.02, 0.02) * self.rendered_scale()
    }
}

/// Linked skull, jaw and floor dimensions for the two serpent recipes.
/// Mesh vertices scale about the skull; child pivots use the same dimensions.
pub(super) struct SerpentHead {
    pub scale: Vec3,
}

impl SerpentHead {
    pub fn resolve(style: Style) -> Self {
        Self {
            scale: if style == Style::Rattlesnake {
                Vec3::new(1.65, 1.45, 1.4)
            } else {
                Vec3::new(1.5, 1.45, 1.45)
            },
        }
    }

    pub fn hinge(&self) -> Vec3 {
        Vec3::new(0.0, -0.025, -0.025) * self.scale
    }

    pub fn tongue(&self) -> Vec3 {
        Vec3::new(0.0, -0.022, 0.205) * self.scale - self.hinge()
    }

    pub fn fangs(&self) -> Vec3 {
        Vec3::new(0.0, -0.016, 0.055 + 1.3f32.cos() * 0.11) * self.scale
    }

    pub fn ground(&self, roll: f32) -> f32 {
        // Support envelopes of the closed cranium, muzzle, jaw and brows.
        // Off-centre brows and a side fall need more than an upright jaw bound.
        let (sin, cos) = roll.sin_cos();
        let envelope = |x: f32, y: f32, rx: f32, ry: f32| {
            x.abs() * self.scale.x * sin.abs() - y * self.scale.y * cos
                + ((rx * self.scale.x * sin).powi(2) + (ry * self.scale.y * cos).powi(2)).sqrt()
        };
        envelope(0.0, 0.028, 0.095, 0.083)
            .max(envelope(0.0, 0.005, 0.074, 0.043))
            .max(envelope(0.0, -0.038, 0.074, 0.023))
            .max(envelope(0.06, 0.052, 0.03, 0.025))
            .max(envelope(0.055, 0.002, 0.05, 0.052))
            + 0.008
    }
}

/// Linked landmarks for a compact ground bird. Both the sculpt and support
/// solver use these vectors; changing leg proportions cannot leave the body
/// suspended at an unrelated height.
pub(super) struct BirdBody;

impl BirdBody {
    pub const HIP: Vec3 = Vec3::new(0.14, -0.1, 0.0);
    pub const UPPER: Vec3 = Vec3::new(0.0, -0.28, 0.07);
    pub const LOWER: Vec3 = Vec3::new(0.0, -0.34, -0.05);
    pub const ANKLE_HEIGHT: f32 = 0.04;
    pub const BODY_HEIGHT: f32 = Self::ANKLE_HEIGHT - Self::HIP.y - Self::UPPER.y - Self::LOWER.y;

    pub fn hip(side: f32) -> Vec3 {
        Vec3::new(Self::HIP.x * side, Self::HIP.y, Self::HIP.z)
    }
}

/// Where the head sits on the torso: just above the shoulder line.
pub(super) const NECK_SEAT: f32 = 0.535;

/// Equine fore/hind landmarks shared by the sculpt and both support paths.
/// Compactness comes from the cannon lengths, while the hock keeps its bend.
pub(super) struct EquineBody {
    pub breadth: f32,
}

impl EquineBody {
    pub const HIP_Y: f32 = -0.10;
    pub const ANKLE_HEIGHT: f32 = 0.078;
    pub const BODY_HEIGHT: f32 = Self::ANKLE_HEIGHT - Self::HIP_Y + 0.38 + 0.30;

    pub fn resolve(look: &Look) -> Self {
        Self {
            breadth: [0.96, 1.0, 1.07][look.v.build as usize],
        }
    }

    pub fn hip(&self, side: f32, front: bool) -> Vec3 {
        Vec3::new(
            side * if front { 0.15 } else { 0.16 } * self.breadth,
            Self::HIP_Y,
            if front { 0.32 } else { -0.34 },
        )
    }

    pub fn upper(front: bool) -> Vec3 {
        Vec3::new(0.0, -0.38, if front { 0.03 } else { -0.09 })
    }

    pub fn lower(front: bool) -> Vec3 {
        Vec3::new(0.0, -0.30, if front { 0.025 } else { 0.12 })
    }
}

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

/// One connected anatomical mass, shared by sculpting and garment contact.
#[derive(Clone, Copy)]
pub(super) struct LimbMass {
    pub center: Vec3,
    pub radii: Vec3,
}

impl LimbMass {
    /// Conservative positive level of the ellipsoid's approximate distance.
    pub fn level_radii(self, margin: f32) -> Vec3 {
        self.radii * (1.0 + margin / self.radii.min_element())
    }
}

/// Four broad masses establish upper attachment, muscle, distal taper and knee.
#[derive(Clone, Copy)]
pub(super) struct LimbContour {
    pub masses: [LimbMass; 4],
    pub blend: f32,
}

impl LimbContour {
    pub fn margin(self) -> f32 {
        self.blend * (self.masses.len() - 1) as f32 * 0.25
    }
}

/// A limb contour shared by the sculpt and its fitting proxies.
/// Ordinary limbs retain the tapered core and belly; authored contours use masses.
#[derive(Clone, Copy)]
pub(super) struct LimbShape {
    pub length: f32,
    pub radius_top: f32,
    pub radius_bottom: f32,
    pub belly: Vec3,
    pub belly_center: Vec3,
    pub contour: Option<LimbContour>,
}

impl LimbShape {
    pub fn new(length: f32, radius_top: f32, radius_bottom: f32, belly: f32, forward: f32) -> Self {
        Self {
            length,
            radius_top,
            radius_bottom,
            belly: Vec3::new(belly, length * 0.32, belly * 1.05),
            belly_center: Vec3::new(0.0, -length * 0.35, forward),
            contour: None,
        }
    }

    fn with_thigh_contour(mut self, build: f32, muscle: f32) -> Self {
        let length = self.length;
        self.contour = Some(LimbContour {
            masses: [
                LimbMass {
                    center: Vec3::new(0.0, -length * 0.06, -0.002),
                    radii: Vec3::new(0.080 * build, length * 0.24, 0.090 * build),
                },
                LimbMass {
                    center: Vec3::new(0.0, -length * 0.35, 0.008),
                    radii: Vec3::new(
                        0.092 * build * muscle,
                        length * 0.45,
                        0.087 * build * muscle,
                    ),
                },
                LimbMass {
                    center: Vec3::new(0.0, -length * 0.66, 0.0),
                    radii: Vec3::new(0.074 * build, length * 0.36, 0.061 * build),
                },
                LimbMass {
                    center: Vec3::new(0.0, -length * 0.965, 0.008),
                    radii: Vec3::new(0.054 * build, length * 0.17, 0.052 * build),
                },
            ],
            blend: 0.012,
        });
        self
    }

    pub fn mesh_bounds(self, padding: f32) -> (Vec3, Vec3) {
        if let Some(contour) = self.contour {
            let mut low = Vec3::splat(f32::INFINITY);
            let mut high = Vec3::splat(f32::NEG_INFINITY);
            for mass in contour.masses {
                let radii = mass.level_radii(contour.margin()) + Vec3::splat(padding);
                low = low.min(mass.center - radii);
                high = high.max(mass.center + radii);
            }
            (low, high)
        } else {
            let radius = self.radius_top.max(self.radius_bottom).max(self.belly.x) + padding;
            (
                Vec3::new(-radius, -self.length - radius, -radius),
                Vec3::splat(radius),
            )
        }
    }
}

/// A constructed shoe last in ankle-local coordinates. Shaft height belongs to
/// BootShape; this last preserves the common supporting sole height.
#[derive(Clone, Copy)]
pub(super) struct BootLast {
    pub build: f32,
    pub heeled: bool,
}

#[derive(Clone, Copy)]
pub(super) struct BootSection {
    pub z: f32,
    pub width: f32,
    pub bottom: f32,
    pub top: f32,
    pub height: f32,
}

impl BootLast {
    pub const FLOOR: f32 = -0.077;

    pub fn sections(self) -> [BootSection; 7] {
        let z = [-0.075, -0.055, 0.0, 0.065, 0.12, 0.17, 0.195];
        let width = [0.006, 0.047, 0.055, 0.062, 0.060, 0.045, 0.006];
        let height = [0.014, 0.062, 0.080, 0.068, 0.038, 0.030, 0.012];
        let bottom = if self.heeled {
            [-0.038, -0.038, -0.032, -0.044, -0.077, -0.077, -0.073]
        } else {
            [-0.074, -0.077, -0.077, -0.077, -0.077, -0.075, -0.071]
        };
        std::array::from_fn(|i| BootSection {
            z: z[i] * self.build,
            width: width[i] * self.build * if self.heeled { 0.95 } else { 1.0 },
            bottom: bottom[i],
            top: bottom[i] + 0.012,
            height: height[i],
        })
    }

    /// Conservative contact ellipsoids cover the constructed sole, counter and
    /// heel; bare feet continue to use their anatomical envelope.
    pub fn contacts(self) -> [(Vec3, Vec3); 3] {
        let b = self.build;
        [
            (
                Vec3::new(0.0, -0.015, -0.028 * b),
                Vec3::new(0.09 * b, 0.09, 0.095 * b),
            ),
            (
                Vec3::new(0.0, -0.020, 0.055 * b),
                Vec3::new(0.10 * b, 0.10, 0.14 * b),
            ),
            (
                Vec3::new(0.0, -0.040, 0.143 * b),
                Vec3::new(0.09 * b, 0.065, 0.075 * b),
            ),
        ]
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
            (0.9, 0.75, 1.12)
        } else {
            (1.04, 0.94, 1.0)
        };
        let profile = BuildProfile::resolve(look.style, look.v.build);
        // Pelvic breadth places the femurs; it does not multiply muscle volume.
        // Lean, sturdy and muscular thighs follow their own build profile so a
        // feminine pelvis does not inflate the whole upper leg.
        let thigh_belly = 0.092 * build * profile.muscle;
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
        let organic_legs = look.v.woman && !matches!(look.style, Style::Guardian | Style::Yeti);
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
            // A restrained upper-leg transition flows from the pelvis into
            // the shared muscle taper and a clearly narrower knee.
            thigh: {
                let shape = LimbShape::new(
                    0.44,
                    0.088 * build * if look.v.woman { 1.02 } else { 1.0 },
                    0.06 * build,
                    thigh_belly,
                    0.008,
                );
                if organic_legs {
                    shape.with_thigh_contour(build, profile.muscle)
                } else {
                    shape
                }
            },
            shin: if look.v.woman {
                LimbShape::new(
                    0.4,
                    (if organic_legs { 0.056 } else { 0.062 }) * build,
                    0.042 * build,
                    0.07 * build,
                    -0.016,
                )
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
