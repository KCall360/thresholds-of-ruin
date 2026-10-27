//! Resolved body dimensions shared by generation, garment fitting and motion.
//! Existing family recipes resolve linked build silhouettes here. Storybook
//! mesh scales enlarge masses separately from the body's joint landmarks.
use super::{Look, Plan, Style, Vec2, Vec3, J};

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
    /// A woman's chest, fitted to this rib cage; garments cut from it.
    pub bust: Option<Bust>,
}

/// How a chest's volume sits on the rib cage. Contour is independent of
/// volume, root spacing and build: the same volume can be broad and shallow
/// or narrow and projected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BustContour {
    /// Balanced upper slope and lower fullness.
    Round,
    /// A wide footprint with modest forward projection and open sternum.
    Broad,
    /// A narrower footprint with more forward projection.
    Projected,
    /// Weight sits lower: a long upper slope above a fuller lower contour.
    Relaxed,
    /// A fuller upper contour above the same attachment.
    UpperFull,
    /// A gently tapering cone, wide-set, with little upper fullness.
    Tapered,
}

/// Shape factors a contour applies to any volume.
struct ContourShape {
    /// Lower mass width and depth.
    width: f32,
    depth: f32,
    /// How far the fullest point sits below its usual height, in fold drops.
    low: f32,
    /// Fullness of the upper slope, 0 a straight ramp to 1 well rounded.
    upper: f32,
    /// Root spacing and attachment footprint.
    spacing: f32,
    footprint: f32,
}

impl BustContour {
    pub const FAMILIES: [Self; 6] = [
        Self::Round,
        Self::Broad,
        Self::Projected,
        Self::Relaxed,
        Self::UpperFull,
        Self::Tapered,
    ];

    fn shape(self) -> ContourShape {
        let (width, depth, low, upper, spacing, footprint) = match self {
            Self::Round => (1.0, 1.0, 0.0, 0.5, 1.0, 1.0),
            Self::Broad => (1.12, 0.66, 0.1, 0.3, 1.1, 1.3),
            Self::Projected => (0.82, 1.32, 0.0, 0.45, 0.95, 0.84),
            Self::Relaxed => (1.06, 0.86, 1.5, 0.1, 1.05, 1.06),
            Self::UpperFull => (1.0, 1.04, -0.8, 1.25, 1.0, 1.0),
            Self::Tapered => (0.88, 0.95, 0.3, 0.1, 1.1, 1.1),
        };
        ContourShape {
            width,
            depth,
            low,
            upper,
            spacing,
            footprint,
        }
    }
}

/// A kind's typical chest: individuals vary about it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BustProfile {
    /// 0 very small to 1 very large.
    pub volume: f32,
    pub contour: BustContour,
}

impl BustProfile {
    pub fn of(style: Style) -> Self {
        use BustContour::*;
        let (volume, contour) = match style {
            Style::Vampire => (0.58, UpperFull),
            Style::Nymph => (0.62, UpperFull),
            Style::Medusa => (0.6, Projected),
            Style::Centaur => (0.55, Projected),
            Style::Orc => (0.68, Broad),
            Style::Troll => (0.72, Relaxed),
            Style::Hobgoblin => (0.42, Broad),
            Style::Zombie => (0.36, Relaxed),
            Style::Leprechaun => (0.3, Tapered),
            Style::Phantom | Style::Wraith => (0.46, Relaxed),
            _ => (0.46, Round),
        };
        Self { volume, contour }
    }
}

/// One woman's chest, in torso coordinates before the Storybook part scale,
/// for her right side (+x); the left mirrors it. Each side is one form
/// attached across the rib cage: a broad base, an upper slope from high on
/// the chest wall and a lower fullness, never a ball set on the chest.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bust {
    pub contour: BustContour,
    /// 0 very small to about 1.15 very large.
    pub volume: f32,
    /// Centre of each side's attachment across the chest wall.
    pub root: f32,
    /// Half-width of that attachment.
    pub footprint: f32,
    /// Where the upper slope leaves the chest wall, and the fold beneath.
    pub top: f32,
    pub fold: f32,
    /// The lower fullness: centre and radii.
    pub center: Vec3,
    pub radii: Vec3,
    /// Radius of the upper slope where it leaves the chest wall, and the
    /// centre it leaves from, set just inside the wall.
    pub slope: f32,
    pub slope_start: Vec3,
    /// Volume scale of each side, left then right.
    pub sides: [f32; 2],
    /// The chest it is fitted to.
    pub wall: ChestWall,
}

/// The rib cage and upper chest a form is fitted to: the two supports of a
/// torso sculpt, as centre and radii. Any torso recipe can describe its own.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChestWall {
    pub ribs: (Vec3, Vec3),
    pub upper: (Vec3, Vec3),
}

impl ChestWall {
    /// A biped's, for shoulders already scaled by build.
    pub fn biped(build: f32, sh: f32) -> Self {
        Self {
            ribs: (
                Vec3::new(0.0, 0.29, -0.014),
                Vec3::new(0.183 * sh, 0.175, 0.119 * build),
            ),
            upper: (
                Vec3::new(0.0, 0.385, -0.047 * build),
                Vec3::new(0.18 * sh, 0.125, 0.09 * build),
            ),
        }
    }

    /// How far forward the bare chest lies at a point across it.
    pub fn front(&self, x: f32, y: f32) -> f32 {
        let ellipse = |(c, r): (Vec3, Vec3)| {
            let t = ((x - c.x) / r.x).powi(2) + ((y - c.y) / r.y).powi(2);
            c.z + r.z * (1.0 - t.min(1.0)).sqrt()
        };
        ellipse(self.ribs).max(ellipse(self.upper))
    }
}

/// The front of a biped's bare chest at a point across it.
pub(crate) fn chest_wall(build: f32, sh: f32, x: f32, y: f32) -> f32 {
    ChestWall::biped(build, sh).front(x, y)
}

impl Bust {
    /// Fit her chest to a chest wall, for shoulders before build scaling.
    pub fn resolve(
        style: Style,
        v: super::Variant,
        build: f32,
        shoulders: f32,
        wall: ChestWall,
    ) -> Self {
        let profile = BustProfile::of(style);
        let contour = match v.bust_form.checked_sub(3) {
            None => profile.contour,
            Some(k) => BustContour::FAMILIES[k as usize % BustContour::FAMILIES.len()],
        };
        let volume = (profile.volume + (v.bust as f32 - 2.0) * 0.2).clamp(0.03, 1.2);
        let s = contour.shape();
        let b = build;
        let sh = shoulders * build;
        // Larger volumes carry their weight lower on the same rib cage.
        let drop = 0.012 + 0.02 * volume;
        let fold = 0.292 - drop * (0.6 + s.low).max(0.15);
        let r = (0.044 + 0.05 * volume) * b;
        let mut radii = Vec3::new(r * s.width, r * 0.92, r * s.depth);
        let projection = (0.033 + 0.082 * volume) * b * s.depth.sqrt();
        let fullest = fold + radii.y * 0.95;
        let root = 0.074 * sh * s.spacing;
        // The fullness turns slightly outward, following the rib cage, but
        // never overhangs its side: a broad form spreads across the chest.
        let x = root + 0.25 * projection;
        radii.x = radii.x.min(wall.ribs.1.x * 0.92 - x);
        let center = Vec3::new(x, fullest, wall.front(x, fullest) + projection - radii.z);
        let lean = match v.bust_lean {
            1 => [1.05, 0.95],
            2 => [0.95, 1.05],
            _ => [1.0, 1.0],
        };
        let top = 0.445;
        let slope = (0.012 + 0.022 * s.upper) * b * (0.7 + 0.6 * volume);
        let start = Vec2::new(root * 0.92, top - 0.02);
        Self {
            contour,
            volume,
            root,
            footprint: (0.068 + 0.026 * volume) * b * s.footprint,
            top,
            fold,
            center,
            radii,
            slope,
            slope_start: start.extend(wall.front(start.x, start.y) - slope * 0.75),
            sides: lean,
            wall,
        }
    }

    fn side_scale(&self, side: f32) -> f32 {
        self.sides[usize::from(side > 0.0)]
    }

    /// The lower fullness of one side (-1 left, 1 right), grown outward.
    pub fn mass(&self, side: f32, grow: f32) -> (Vec3, Vec3) {
        let k = self.side_scale(side);
        let c = self.center;
        let radii = self.radii * k.cbrt();
        // A fuller side sits a little lower, its front kept on the same wall.
        let y = c.y - (radii.y - self.radii.y);
        let z = c.z + (radii.z - self.radii.z) * 0.5;
        (Vec3::new(side * c.x, y, z), radii + Vec3::splat(grow))
    }

    /// The fullest, most forward point of one side.
    pub fn apex(&self, side: f32) -> Vec3 {
        let (c, r) = self.mass(side, 0.0);
        c + Vec3::Z * r.z
    }
}

/// A centaur woman's chest, fitted to the rider's own rib cage.
pub(super) fn centaur_bust(look: &Look) -> Option<Bust> {
    look.v.woman.then(|| {
        let w = 0.88;
        let wall = ChestWall {
            ribs: (Vec3::new(0.0, 0.3, 0.005), Vec3::new(0.2 * w, 0.17, 0.13)),
            upper: (Vec3::new(0.0, 0.44, -0.01), Vec3::new(0.2 * w, 0.1, 0.11)),
        };
        Bust::resolve(Style::Centaur, look.v, 1.0, 0.2 * w / 0.183, wall)
    })
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
            // Her own figure narrows the waist against fuller hips, or not.
            // Her kind leans the roll toward its typical figure.
            let lean: i8 = match look.style {
                Style::Nymph | Style::Vampire | Style::Medusa => 1,
                Style::Orc | Style::Troll | Style::Hobgoblin => -1,
                _ => 0,
            };
            let f = (look.v.figure.min(2) as i8 + lean).clamp(0, 2) as usize;
            (
                0.9,
                0.75 * [1.08, 1.0, 0.91][f],
                1.12 * [0.96, 1.0, 1.03][f],
            )
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
            bust: (look.v.woman && !matches!(look.style, Style::Guardian | Style::Yeti)).then(
                || {
                    Bust::resolve(
                        look.style,
                        look.v,
                        build,
                        shoulders,
                        ChestWall::biped(build, shoulders * build),
                    )
                },
            ),
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
