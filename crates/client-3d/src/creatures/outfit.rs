//! What a humanoid wears, slot by slot, apart from the body beneath it.
//!
//! Every clothed kind has its own outfit, drawn from its concept sheet, and
//! that is what it wears unless dressed otherwise. Supported garments and
//! armour are cut from the wearer's resolved anatomy, never from its kind.
//! The wardrobe catalog resolves body-specific preview fallbacks; exact requests
//! reject substitutions. Changing one slot changes only the parts
//! that slot dresses, so a look can be redressed while it moves.
use super::*;
mod embroidery;
mod equipment;
mod native;
mod presets;
pub(crate) use equipment::{BodyItem, Composition, Equipment};
mod royal;
mod studies;
mod sylvan;
pub(crate) use embroidery::Motif;

/// A garment pigment or a wearer-owned skin reference. Resolve at fitting time,
/// never by comparing RGB values or rewriting the stored recipe on transfer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pigment {
    Fixed(Vec3),
    Skin(f32),
}

impl Pigment {
    pub fn resolve(self, skin: Vec3) -> Vec3 {
        match self {
            Self::Fixed(color) => color,
            Self::Skin(shade) => skin * shade,
        }
    }
}

/// What covers the arms below the shoulder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Arms {
    Bare,
    Sleeves(Vec3),
    /// Pauldrons over rerebrace lames, elbow cops and vambraces.
    Plate,
    /// A sleeve with a cut of its own.
    Styled(Sleeve),
}

impl Arms {
    /// Whether the upper arm, and the shoulder above it, are bare.
    pub fn bare(self) -> bool {
        match self {
            Arms::Bare => true,
            Arms::Styled(sleeve) => matches!(
                sleeve.cut,
                SleeveCut::Puff | SleeveCut::Gauntlet | SleeveCut::Draped | SleeveCut::OffShoulder
            ),
            _ => false,
        }
    }
}

/// How a styled sleeve is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SleeveCut {
    /// Only a short puff over the shoulder; the arm bare below.
    Puff,
    /// Fitted to the elbow, then flaring into a wide bell or ruffle.
    Bell,
    /// The upper arm bare, the forearm in a fitted gauntlet with a sheer
    /// drape hanging from it.
    Gauntlet,
    /// Bare upper arm and fitted forearm with an opaque hanging drape.
    Draped,
    /// Off the shoulder: a gathered band round the upper arm.
    OffShoulder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sleeve {
    pub color: Vec3,
    pub cut: SleeveCut,
    /// A puff over the shoulder as well, above a longer sleeve.
    pub puff: bool,
    /// Contrasting lining revealed between gathered strips of the puff.
    pub puff_lining: Option<Vec3>,
    /// Its puffs and bands are gauze, seen through.
    pub sheer: bool,
    /// The bell, ruffle or drape at its end, if of another colour.
    pub flare: Option<Vec3>,
}

/// What covers the legs from hip to ankle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Legs {
    /// Skin or hose fitted to the leg.
    Fitted(Pigment),
    /// Cloth eased off the leg, gathered below the knee.
    Trousers(Vec3),
    /// Mail beneath cuisses and knee cops.
    Plate,
    /// Bare legs: the torso garment is cut high over the seat.
    Bare(Pigment),
}

/// Footwear, fitted to the wearer's feet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Feet {
    /// Bare feet with wrappings at the ankle, in the skin's colour.
    Bare(Pigment),
    Boots(Boots),
    /// Low shoes or sandals with the leg bare above them.
    Shoes(Shoes),
}

/// Pumps or sandals: the foot closed in, or bare on its sole and held by
/// straps, which may lace up the leg.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shoes {
    pub color: Vec3,
    pub heel: bool,
    /// A sandal: the foot bare on its sole.
    pub sandal: bool,
    /// How far up the shin its straps lace, 0 at the ankle to 1 the knee.
    pub straps: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Boots {
    pub color: Vec3,
    /// Knee-high rather than mid-calf.
    pub tall: bool,
    pub heel: bool,
    /// Strap-and-buckle bands round the shaft.
    pub buckles: bool,
    /// A turned-down cuff at the top of the shaft.
    pub cuffed: bool,
}

/// What is worn on the head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Headwear {
    Bare,
    /// Open pointed filigree crown fitted over the wearer's hair.
    Crown {
        metal: Vec3,
        gem: Vec3,
    },
    /// An open metal cap with paired feather fans.
    WingedHelm {
        metal: Vec3,
        feathers: Vec3,
    },
    /// A deep hood over the head, perhaps ending in a long soft point.
    Hood {
        color: Vec3,
        pointed: bool,
    },
    /// A hood pushed back as a cowl over the shoulders.
    HoodDown(Vec3),
    /// A closed plumed helm.
    Helm,
    /// A pirate's three-cornered hat, its brim turned up and edged.
    Tricorn {
        color: Vec3,
        trim: Vec3,
    },
    /// A tall bent cone over a wide brim, banded and bowed.
    WitchHat {
        color: Vec3,
        band: Vec3,
    },
    /// A wreath of leaves round the head.
    Wreath(Vec3),
    /// A band round the brow, narrow or broad, perhaps with a jewel.
    Circlet {
        color: Vec3,
        gem: Option<Vec3>,
        broad: bool,
    },
    /// A small cap on the crown with a sheer veil falling behind.
    Veiled {
        cap: Vec3,
        veil: Vec3,
    },
}

/// Authored opening of a jerkin or coat, fitted against the receiving body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShellOpening {
    Closed,
    /// Follow the undergarment neckline, with a rounded opening when closed.
    ShapedNeck,
    /// A coat falling open from its collar along the undergarment plunge.
    PlungingCoat,
    /// A long open front exposing the waistcoat beneath it.
    OpenCoat,
}

/// A fitted layer over the tunic.
// Recipes are small Copy values baked once per appearance. Keep a complete
// composable garment on the shell rather than allocating each optional layer.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Outer {
    /// A jerkin, vest or coat, ending at `top` on the chest.
    Shell {
        color: Vec3,
        top: f32,
        sleeveless: bool,
        leather: bool,
        /// Padded cloth stitched in diamonds: a gambeson.
        quilted: bool,
        /// Cut off beneath her chest rather than at `top`: a laced corset
        /// that lifts and frames whatever chest it is fitted under.
        underbust: bool,
        opening: ShellOpening,
        /// Its free hem below the belt, including its cut, pigment and trim.
        peplum: Option<Skirt>,
    },
    /// A plate cuirass with its gorget and gilt-edged tabard panels.
    Cuirass,
}

/// The opening belongs to the garment, independently of the wearer species.
/// Its cut is resolved against the wearer's own chest, so one garment
/// plunges to the sternum on any chest while keeping the fullness covered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Neckline {
    Closed,
    /// A V down the sternum. `depth` runs from 0 at the base of the throat
    /// to 1 at the fold beneath the chest; `width` is the opening's
    /// half-width at the collarbones, as a share of the shoulders. A wrap's
    /// left panel crosses over beneath the point to the right of the waist.
    Plunge {
        depth: f32,
        width: f32,
        wrap: bool,
    },
    /// A teardrop opening over the valley of her chest beneath a closed
    /// collar, as deep as a plunge of this `depth`.
    Keyhole {
        depth: f32,
        width: f32,
    },
}

impl Neckline {
    /// A split linen V: open to the upper sternum.
    pub const SPLIT: Self = Self::Plunge {
        depth: 0.6,
        width: 0.15,
        wrap: false,
    };
    /// The adventurer's deep V.
    pub const DEEP: Self = Self::Plunge {
        depth: 0.78,
        width: 0.16,
        wrap: false,
    };
    /// A deep wrap, crossed over and tied at the waist.
    pub const WRAP: Self = Self::Plunge {
        depth: 0.82,
        width: 0.16,
        wrap: true,
    };
    /// A court or dramatic plunge, nearly to the fold.
    pub const COURT: Self = Self::Plunge {
        depth: 0.9,
        width: 0.17,
        wrap: false,
    };

    fn fitted(woman: bool, cut: Self) -> Self {
        if woman {
            cut
        } else {
            Self::Closed
        }
    }

    /// One of the cuts a garment is made in, by the wearer's own roll.
    fn chosen(woman: bool, roll: u8, cuts: &[Self]) -> Self {
        Self::fitted(woman, cuts[roll as usize % cuts.len()])
    }
}

/// How the torso garment carries a chest beneath it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Support {
    /// Knit, leaves or rags: the garment follows every contour.
    None,
    /// Loose cloth: it bridges below the chest rather than lifting it.
    Loose,
    /// Leather or a fitted bodice: it falls from the fullness to the ribs.
    Structured,
}

impl Support {
    /// How much of the chest's depth the garment spans beneath it.
    pub fn share(self) -> Option<f32> {
        match self {
            Support::None => None,
            Support::Loose => Some(0.55),
            Support::Structured => Some(0.8),
        }
    }

    /// How readily the chest beneath gives as she moves.
    pub fn give(self) -> f32 {
        match self {
            Support::None => 1.0,
            Support::Loose => 0.8,
            Support::Structured => 0.45,
        }
    }
}

/// How much of the torso a garment covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cover {
    /// Exposed torso, with accessories such as a harness composed separately.
    Bare,
    /// The whole torso, opening only where the bodice is cut.
    Full,
    /// Cropped just beneath her chest, the midriff bare; its lower edge is
    /// cut like a hem, even or in points.
    Crop(Hem),
    /// Only her chest, in cups fitted to it, a band beneath and strings tied
    /// at the neck; the rest is bare. A man goes bare-chested.
    Bikini,
}

/// What holds a bodice up, and so how much of the shoulders and upper back
/// it covers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Straps {
    /// Over both shoulders: sleeves or straps.
    Shoulders,
    /// Two front panels rising to tie at the neck; shoulders and back bare.
    Halter,
    /// Over her right shoulder only, the top edge falling diagonally to
    /// beneath her left arm.
    OneShoulder,
    /// None: a sweetheart edge across her chest, everything above it bare.
    Strapless,
    /// Low curved cups with a deep center dip and bare shoulders.
    Sweetheart,
    /// Separate shoulder-to-waist strips, with open center and torso sides.
    Panels { width: f32 },
}

/// The front of a torso garment, composed as one: how it opens, how that
/// edge is finished, whether it is laced across, and how it carries a chest.
/// Any wardrobe recipe can wear any bodice; the cut resolves on the wearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bodice {
    /// How much of the torso the garment covers.
    pub cover: Cover,
    pub cut: Neckline,
    /// The binding along the opening.
    pub edge: Vec3,
    /// Cords laced across the opening, if it is laced.
    pub lacing: Option<Vec3>,
    pub support: Support,
    pub straps: Straps,
    pub pattern: Pattern,
    /// A separately authored male opening; omitted recipes retain their closed front.
    pub male_cut: Option<Neckline>,
}

impl Bodice {
    /// The same front cut for a wearer: a man's is closed over both
    /// shoulders in the same cloth.
    pub fn fitted(self, woman: bool) -> Self {
        if woman {
            self
        } else {
            Self {
                cover: if matches!(self.cover, Cover::Bare | Cover::Bikini) {
                    self.cover
                } else {
                    Cover::Full
                },
                cut: self.male_cut.unwrap_or(Neckline::Closed),
                lacing: self.male_cut.and(self.lacing),
                straps: Straps::Shoulders,
                ..self
            }
        }
    }

    /// A plain closed front; recipes set what differs.
    pub const PLAIN: Bodice = Bodice {
        cover: Cover::Full,
        cut: Neckline::Closed,
        edge: Vec3::ZERO,
        lacing: None,
        support: Support::Loose,
        straps: Straps::Shoulders,
        pattern: Pattern::Plain,
        male_cut: None,
    };
}

/// How a free hem is cut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Hem {
    Even,
    /// Cut in points round the hem, each `depth` of the length deep: a
    /// handkerchief or ragged hem.
    Points {
        count: u8,
        depth: f32,
    },
    /// Shorter in front by this share of its length, long behind.
    HighLow(f32),
}

impl Hem {
    /// The share of its length the cloth hangs to at an angle round it,
    /// 0 at the front.
    pub fn reach(self, angle: f32) -> f32 {
        match self {
            Hem::Even => 1.0,
            Hem::Points { count, depth } => {
                let phase = (angle * count as f32 / std::f32::consts::TAU).rem_euclid(1.0);
                1.0 - depth * (1.0 - (2.0 * phase - 1.0).abs())
            }
            Hem::HighLow(rise) => 1.0 - rise * angle.cos().max(0.0).powi(2),
        }
    }
}

/// What a garment's cloth looks like beyond its colour: woven, printed or
/// stitched, the same on any garment that wears it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Pattern {
    Plain,
    /// A repeatable embroidered motif shared by any cloth surface.
    Brocade {
        thread: Vec3,
        motif: Motif,
    },
    /// A big cat's rosettes in this colour.
    Leopard(Vec3),
    /// Small scattered dots in this colour.
    Dots(Vec3),
    /// Fine vertical ribs catching the light: lamé or pleated silk.
    Ribbed,
}

impl Pattern {
    /// The cloth's colour at a point on it.
    pub fn paint(self, base: Vec3, at: Vec3) -> Vec3 {
        let cell = |scale: f32| {
            let q = at / scale;
            let c = q.round();
            let jitter = Vec3::new(
                crate::props::rand(c.x.to_bits() as u64 ^ c.y.to_bits() as u64, 1) - 0.5,
                crate::props::rand(c.y.to_bits() as u64 ^ c.z.to_bits() as u64, 2) - 0.5,
                0.0,
            ) * 0.5;
            (q - c - jitter).length()
        };
        match self {
            Pattern::Plain => base,
            Pattern::Brocade { thread, motif } => {
                if motif.stitch(at) {
                    thread
                } else {
                    base
                }
            }
            Pattern::Leopard(spot) => {
                // A ring of dark spots round a darker heart.
                let d = cell(0.05);
                if (0.22..0.36).contains(&d) {
                    spot
                } else if d < 0.18 {
                    base * 0.8
                } else {
                    base
                }
            }
            Pattern::Dots(dot) => {
                if cell(0.022) < 0.16 {
                    dot
                } else {
                    base
                }
            }
            Pattern::Ribbed => base * (0.82 + 0.3 * (at.x * 310.0).sin().abs()),
        }
    }
}

/// A layer beneath a skirt, showing below its hem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Layer {
    pub color: Vec3,
    pub sheer: bool,
    /// How much longer than the skirt over it.
    pub longer: f32,
    pub hem: Hem,
    /// Independent openings and fullness; None follows the parent cut.
    pub split: Option<f32>,
    pub fullness: Option<f32>,
    pub pattern: Pattern,
    pub trim: Option<Vec3>,
    pub panel: Option<(Vec3, f32)>,
}

impl Layer {
    /// A petticoat a little longer than the skirt, evenly hemmed.
    pub const fn petticoat(color: Vec3, sheer: bool) -> Self {
        Self {
            color,
            sheer,
            longer: 0.035,
            hem: Hem::Even,
            split: None,
            fullness: None,
            pattern: Pattern::Plain,
            trim: None,
            panel: None,
        }
    }

    /// Resolve once: mesh construction and motion consume the same garment.
    pub fn resolve(self, parent: Skirt) -> Skirt {
        Skirt {
            color: self.color,
            length: parent.length + self.longer,
            hem: self.hem,
            split: self.split.unwrap_or(parent.split),
            fullness: self.fullness.unwrap_or(parent.fullness),
            pattern: self.pattern,
            trim: self.trim,
            panel: self.panel,
            sheer: self.sheer,
            layers: [None, None],
            ..parent
        }
    }
}

/// A free cloth panel, including the construction that travels with it.
/// Recipes start from [`Skirt::PLAIN`] and set what differs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Skirt {
    pub color: Vec3,
    pub length: f32,
    pub split: f32,
    pub trim: Option<Vec3>,
    pub cutaway: bool,
    /// Narrow central drape between a pair of thigh openings, angular half-width.
    pub center_panel: Option<f32>,
    /// Explicit level waist seam; otherwise paired panels use the raised royal cut.
    pub waistline: Option<f32>,
    pub hem: Hem,
    /// How full it flares, 1 the ordinary cut.
    pub fullness: f32,
    /// A front panel of another colour down its middle, as wide as this
    /// share of the way round.
    pub panel: Option<(Vec3, f32)>,
    /// Layers beneath it: petticoats, tiers and underskirts.
    pub layers: [Option<Layer>; 2],
    /// Gauze or chiffon, seen through.
    pub sheer: bool,
    pub pattern: Pattern,
}

impl Skirt {
    pub fn waist_height(self, angle: f32) -> f32 {
        self.waistline.unwrap_or_else(|| {
            if self.center_panel.is_some() {
                0.135 - 0.075 * angle.cos().max(0.0).powi(4)
            } else {
                0.04
            }
        })
    }

    pub const PLAIN: Skirt = Skirt {
        color: Vec3::ZERO,
        length: 0.0,
        split: 0.0,
        trim: None,
        cutaway: false,
        center_panel: None,
        waistline: None,
        hem: Hem::Even,
        fullness: 1.0,
        panel: None,
        layers: [None, None],
        sheer: false,
        pattern: Pattern::Plain,
    };
}

/// What is worn round the neck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Neck {
    /// Fine chain and a pendant resting on the upper sternum.
    Pendant { metal: Vec3, gem: Vec3 },
    /// A snug band.
    Choker(Vec3),
    /// A choker with a jewel at the throat.
    Jewel { band: Vec3, gem: Vec3 },
    /// A broad collar over the collarbones, fringed with beads.
    Collar { metal: Vec3, beads: Vec3 },
    /// Claws or teeth strung on a cord.
    Teeth { cord: Vec3 },
    /// A stiff collar standing up round the neck.
    Stand(Vec3),
}

/// What is worn round the waist over the garments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Waist {
    /// Fitted pointed framework with hanging jewels.
    Jeweled { metal: Vec3, gem: Vec3 },
    /// A knotted sash with tails hanging at one hip.
    Sash(Vec3),
    /// A low V belt hung with coins.
    Coins(Vec3),
    /// Cords crossing round the waist, tied in front.
    Crossed(Vec3),
    /// Two broad leather straps fitted across the waist.
    CrossedBelts(Vec3),
}

impl Waist {
    /// Fitted waist pieces follow the surface they were fitted to; low hip
    /// ornaments stay with the skirt. Placement and animation must agree.
    pub(crate) fn attachment(self) -> J {
        match self {
            Self::Sash(_) | Self::Crossed(_) | Self::CrossedBelts(_) => J::Torso,
            Self::Jeweled { .. } | Self::Coins(_) => J::Hips,
        }
    }
}

/// Solid, overlapping trim that can edge a shoulder, boot or another fitted rim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FringeKind {
    Fur,
    Feathers,
    Leaves,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fringe {
    pub kind: FringeKind,
    pub color: Vec3,
    pub accent: Vec3,
    pub length: f32,
}

/// Colors and insignia belong to the tabard, independently of its wearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Heraldry {
    pub field: Vec3,
    pub edge: Vec3,
    pub emblem: Vec3,
    pub mark: u8,
}

/// The small things an outfit adds, each fitted to any wearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Trimmings {
    pub heraldry: Option<Heraldry>,
    pub shoulders: Option<Fringe>,
    pub boot_tops: Option<Fringe>,
    pub neck: Option<Neck>,
    /// Bands round the upper arms.
    pub armlets: Option<Vec3>,
    pub bracelets: Option<Vec3>,
    pub earrings: Option<(Vec3, Vec3)>,
    pub waist: Option<Waist>,
    /// Stiff pointed shoulders standing up over the arms.
    pub peaks: Option<Vec3>,
    /// A cloth tie knotted round one thigh.
    pub garter: Option<Vec3>,
}

impl Trimmings {
    pub const NONE: Trimmings = Trimmings {
        heraldry: None,
        shoulders: None,
        boot_tops: None,
        neck: None,
        armlets: None,
        bracelets: None,
        earrings: None,
        waist: None,
        peaks: None,
        garter: None,
    };
}

/// A closed shoulder-to-waist band fitted over the wearer's torso layers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Baldric {
    pub color: Vec3,
    pub reverse: bool,
    pub buckle: bool,
}

/// A torso accessory chosen by the outfit, with its authored materials.
/// Construction names allow the same ornament to dress another kind of body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TorsoOrnament {
    LacedFront {
        cord: Vec3,
    },
    Placket {
        color: Vec3,
    },
    RivetedVest {
        strap: Vec3,
        rivets: Vec3,
    },
    StitchedFront {
        thread: Vec3,
    },
    HighCollar {
        collar: Vec3,
        pads: Vec3,
        cravat: Vec3,
        buttons: Vec3,
    },
    Corset {
        metal: Vec3,
        gem: Vec3,
    },
    Harness {
        strap: Vec3,
        ring: Vec3,
        shoulder_straps: Option<Vec3>,
    },
    StockAndButtons {
        stock: Vec3,
        buttons: Vec3,
        belt: Vec3,
    },
    LeafBodice {
        leaves: Vec3,
        pin: Vec3,
    },
    NeckBandAndDisc {
        metal: Vec3,
    },
    KnottedSash {
        sash: Vec3,
        scarf: Option<Vec3>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pouch {
    pub color: Vec3,
}

/// A gathered neck drape fitted to the wearer, optionally with a hanging tail.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NeckDrape {
    pub color: Vec3,
    pub tail: bool,
    /// Hood bases are replaced with the hood when an independent helm is worn.
    pub covered_by_helm: bool,
}

/// Authored cloak appearance and cut, independent of its wearer's anatomy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Cloak {
    pub exterior: Vec3,
    pub lining: Vec3,
    pub length: f32,
    /// Torn hem cut, retained when the garment moves to another wearer.
    pub ragged: bool,
}

impl Cloak {
    pub const fn new(exterior: Vec3, lining: Vec3) -> Self {
        Self {
            exterior,
            lining,
            length: 1.08,
            ragged: false,
        }
    }
}

/// A continuous leg-fitted robe, including its sash and bell-cuff trim.
/// Authored colors and fold variation travel with the garment, not its wearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FittedRobe {
    pub color: Vec3,
    pub trim: Vec3,
    pub fold_seed: u8,
}

/// Independently composable sewn or attached garment details. The wearer
/// supplies fitting dimensions; these recipes retain authored pigments and cut.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct GarmentDetails {
    pub hip: Option<HipDecoration>,
    pub shoulder_discs: Option<Vec3>,
    pub sleeve_tatters: Option<Vec3>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum HipDecoration {
    Loincloth { cloth: Vec3, edge: Vec3, ring: Vec3 },
    RaggedApron { cloth: Vec3, apron: Vec3 },
    BucklePouch { metal: Vec3, pouch: Vec3 },
    Leaves { cloth: Vec3, trim: Vec3 },
    TornHem { length: f32, cloth: Vec3 },
}

/// One humanoid's clothes and armour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Outfit {
    pub head: Headwear,
    pub tunic: Vec3,
    /// The tunic's front: its opening, edging, lacing and support.
    pub bodice: Bodice,
    /// Binding or gorget pigment selected with the garment.
    pub collar_edge: Vec3,
    pub robe: Option<FittedRobe>,
    pub details: GarmentDetails,
    pub neck_drape: Option<NeckDrape>,
    /// Bound edges of a sleeveless bodice, if the garment has them.
    pub armhole_edge: Option<Vec3>,
    pub torso_ornament: Option<TorsoOrnament>,
    pub pouch: Option<Pouch>,
    /// A leather belt with its buckle at the waist.
    pub belt: bool,
    /// Jewellery and other small pieces.
    pub trimmings: Trimmings,
    pub baldric: Option<Baldric>,
    pub arms: Arms,
    pub cuffs: Option<Vec3>,
    pub hands: Pigment,
    pub legs: Legs,
    pub feet: Feet,
    pub outer: Option<Outer>,
    /// The tunic's free lower panels.
    pub skirt: Option<Skirt>,
    /// A cloak and its lining.
    pub cloak: Option<Cloak>,
}

/// Outfits any humanoid can be dressed in, beside its own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Wardrobe {
    /// What its kind wears on its concept sheet.
    #[default]
    Native,
    /// Tunic, trousers and plain boots.
    Commoner,
    /// A leather jerkin over a tunic, a cloak and tall buckled boots.
    Ranger,
    /// Full articulated plate over mail, with a tabard and a leather baldric.
    Knight,
    /// A long gown or coat-robe, fitted sleeves and a cloak.
    Courtier,
    /// A fitted one-piece with no structure of its own: sleeveless, scoop
    /// fronted and cut high over bare legs. It shows the figure and its
    /// motion plainly, for studying both.
    Leotard,
    /// A bikini top and bottom under a long sarong slit to the hip.
    Sarong,
    /// A low chemise cinched by a laced under-chest corset, and a long skirt.
    Tavern,
    /// A black sleeveless bodice with a stand collar, pointed shoulders and
    /// a pointed peplum, a slit pointed skirt, a red cape and heeled boots.
    Queen,
    /// A gold halter bodice with a keyhole, a broad collar, a dark column
    /// skirt with a gold panel, gauntlets with sheer drapes, and a headband.
    Pharaoh,
    /// A white plunging chiton over a long sheer high-low layer, gold cords
    /// at the waist, armlets, a laurel wreath and laced sandals.
    Goddess,
    /// A velvet gown open over a pale laced underdress, puffed and belled
    /// sleeves, a cap with a veil and heeled shoes.
    Juliet,
    /// A one-shoulder leopard crop top, a pointed leopard skirt, a tooth
    /// necklace and a thigh tie, barefoot.
    Jungle,
    /// A jewelled bikini top under off-shoulder drapes, a coin belt, a
    /// sheer slit skirt, a circlet and heeled sandals.
    Genie,
    /// A blue sweetheart dress with puffed sleeves and layered petticoats.
    Princess,
    /// A purple sweetheart gown with detached sleeves, two thigh slits,
    /// lavender lining, gold embroidery and a jeweled crown. Female only.
    RoyalPrincess,
    /// Green embroidered open panels, dark trousers, drapes and tall boots.
    SylvanDuelist,
    /// A laced black bodice over red, belled sleeves, a broad belt and a
    /// sash, a ragged red skirt, a tricorn and cuffed boots.
    Pirate,
    /// A plunging halter over a bare midriff, a belt and shoulder strap, a
    /// long slit skirt, bracers and gladiator sandals.
    Maiden,
    /// A laced corset bodice, sheer puffed sleeves, a tiered skirt, a choker,
    /// a witch's hat and heeled sandals.
    Witch,
    ElvenRanger,
    Rogue,
    Paladin,
    Barbarian,
    Valkyrie,
    Cleric,
    Druid,
    Bard,
    VampireNoble,
    Necromancer,
    Desert,
}

impl Wardrobe {
    pub const ALL: [Wardrobe; 31] = [
        Wardrobe::Native,
        Wardrobe::Commoner,
        Wardrobe::Ranger,
        Wardrobe::Knight,
        Wardrobe::Courtier,
        Wardrobe::Leotard,
        Wardrobe::Sarong,
        Wardrobe::Tavern,
        Wardrobe::Queen,
        Wardrobe::Pharaoh,
        Wardrobe::Goddess,
        Wardrobe::Juliet,
        Wardrobe::Jungle,
        Wardrobe::Genie,
        Wardrobe::Princess,
        Wardrobe::RoyalPrincess,
        Wardrobe::SylvanDuelist,
        Wardrobe::Pirate,
        Wardrobe::Maiden,
        Wardrobe::Witch,
        Wardrobe::ElvenRanger,
        Wardrobe::Rogue,
        Wardrobe::Paladin,
        Wardrobe::Barbarian,
        Wardrobe::Valkyrie,
        Wardrobe::Cleric,
        Wardrobe::Druid,
        Wardrobe::Bard,
        Wardrobe::VampireNoble,
        Wardrobe::Necromancer,
        Wardrobe::Desert,
    ];

    /// The eleven paired character-study recipes, in reference-sheet order.
    pub const STUDIES: [Wardrobe; 11] = [
        Wardrobe::ElvenRanger,
        Wardrobe::Rogue,
        Wardrobe::Paladin,
        Wardrobe::Barbarian,
        Wardrobe::Valkyrie,
        Wardrobe::Cleric,
        Wardrobe::Druid,
        Wardrobe::Bard,
        Wardrobe::VampireNoble,
        Wardrobe::Necromancer,
        Wardrobe::Desert,
    ];

    pub fn parse(name: &str) -> Option<Wardrobe> {
        let key: String = name
            .chars()
            .filter(|c| !matches!(c, '-' | '_' | ' '))
            .collect();
        Self::ALL
            .into_iter()
            .find(|w| format!("{w:?}").eq_ignore_ascii_case(&key))
    }

    /// Interactive preview policy. Exact requests must reject a changed recipe;
    /// they must not silently accept the substitute used when browsing outfits.
    pub(crate) fn recipe_for(self, woman: bool) -> Self {
        match (self, woman) {
            (Self::Princess | Self::RoyalPrincess, false) => Self::Courtier,
            (Self::SylvanDuelist, false) => Self::Ranger,
            _ => self,
        }
    }

    /// The next outfit along the rail, wrapping round.
    pub fn next(self) -> Wardrobe {
        let at = Self::ALL.iter().position(|&w| w == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }
}

/// Whether a look wears separable clothes: carved stone and shaggy fur
/// retain specialized body recipes. Casters can exchange their native robes.
pub(crate) fn dresses(l: &Look) -> bool {
    l.plan == Plan::Biped && !matches!(l.style, Style::Guardian | Style::Yeti)
}

impl Outfit {
    /// What a look wears: its own kind's outfit unless it was dressed in
    /// another.
    pub fn worn(l: &Look) -> Outfit {
        Composition::resolve(l).outfit
    }

    /// An outfit from the wardrobe, cut for this wearer.
    pub fn from_wardrobe(w: Wardrobe, l: &Look) -> Outfit {
        presets::compose(w.recipe_for(l.woman()), l)
    }

    /// A plain outfit for this wearer to build on: bare-headed, a closed
    /// plain tunic in her own cloth, bare arms and legs, barefoot, with
    /// nothing worn over it. Recipes set what differs.
    pub fn base(l: &Look) -> Outfit {
        Outfit {
            head: Headwear::Bare,
            tunic: l.cloth,
            bodice: Bodice::PLAIN,
            collar_edge: l.cloth * 0.8,
            robe: None,
            details: GarmentDetails::default(),
            neck_drape: None,
            armhole_edge: None,
            torso_ornament: None,
            pouch: None,
            belt: false,
            trimmings: Trimmings::NONE,
            baldric: None,
            arms: Arms::Bare,
            cuffs: None,
            hands: Pigment::Skin(1.0),
            legs: Legs::Bare(Pigment::Skin(1.0)),
            feet: Feet::Bare(Pigment::Skin(1.0)),
            outer: None,
            skirt: None,
            cloak: None,
        }
    }

    /// Each kind's own outfit, as drawn on its concept sheet.
    pub fn native(l: &Look) -> Outfit {
        native::compose(l)
    }

    /// The colour the upper arms show: skin, sleeve or steel.
    pub fn sleeve(&self, skin: Vec3) -> Vec3 {
        match self.arms {
            Arms::Bare => skin,
            Arms::Sleeves(color) => color,
            Arms::Plate => STEEL,
            Arms::Styled(sleeve) => match sleeve.cut {
                SleeveCut::Bell => sleeve.color,
                SleeveCut::Puff
                | SleeveCut::Gauntlet
                | SleeveCut::Draped
                | SleeveCut::OffShoulder => skin,
            },
        }
    }

    /// The colour the forearms show: skin, sleeve, gauntlet or steel.
    pub fn forearm(&self, skin: Vec3) -> Vec3 {
        match self.arms {
            Arms::Styled(sleeve) => match sleeve.cut {
                SleeveCut::Bell | SleeveCut::Gauntlet | SleeveCut::Draped => sleeve.color,
                SleeveCut::Puff | SleeveCut::OffShoulder => skin,
            },
            _ => self.sleeve(skin),
        }
    }

    /// The colour the legs show: skin, hose, cloth or the mail under plate.
    pub fn pants(&self, skin: Vec3) -> Vec3 {
        match self.legs {
            Legs::Fitted(color) | Legs::Bare(color) => color.resolve(skin),
            Legs::Trousers(color) => color,
            Legs::Plate => IRON,
        }
    }

    /// The colour over the seat: the legwear's, or the torso garment's
    /// where it is cut high over bare legs.
    pub fn seat(&self, skin: Vec3) -> Vec3 {
        match self.legs {
            Legs::Bare(_) => self.tunic,
            _ => self.pants(skin),
        }
    }

    /// The colour of the boots, or the bare feet.
    pub fn boot(&self, skin: Vec3) -> Vec3 {
        match self.feet {
            Feet::Bare(color) => color.resolve(skin),
            Feet::Boots(boots) => boots.color,
            Feet::Shoes(shoes) => shoes.color,
        }
    }
}
