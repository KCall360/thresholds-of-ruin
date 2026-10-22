//! What a humanoid wears, slot by slot, apart from the body beneath it.
//!
//! Every clothed kind has its own outfit, drawn from its concept sheet, and
//! that is what it wears unless dressed otherwise. Any outfit fits any
//! humanoid body: garments and armour are cut from the wearer's resolved
//! anatomy, never from its kind. Changing one slot changes only the parts
//! that slot dresses, so a look can be redressed while it moves.
use super::*;

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
                SleeveCut::Puff | SleeveCut::Gauntlet | SleeveCut::OffShoulder
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
    /// Off the shoulder: a gathered band round the upper arm.
    OffShoulder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sleeve {
    pub color: Vec3,
    pub cut: SleeveCut,
    /// A puff over the shoulder as well, above a longer sleeve.
    pub puff: bool,
    /// Its puffs and bands are gauze, seen through.
    pub sheer: bool,
    /// The bell, ruffle or drape at its end, if of another colour.
    pub flare: Option<Vec3>,
}

/// What covers the legs from hip to ankle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Legs {
    /// Skin or hose fitted to the leg.
    Fitted(Vec3),
    /// Cloth eased off the leg, gathered below the knee.
    Trousers(Vec3),
    /// Mail beneath cuisses and knee cops.
    Plate,
    /// Bare legs: the torso garment is cut high over the seat.
    Bare(Vec3),
}

/// Footwear, fitted to the wearer's feet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Feet {
    /// Bare feet with wrappings at the ankle, in the skin's colour.
    Bare(Vec3),
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

/// A fitted layer over the tunic.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl Bodice {
    /// The same front cut for a wearer: a man's is closed over both
    /// shoulders in the same cloth.
    pub fn fitted(self, woman: bool) -> Self {
        if woman {
            self
        } else {
            Self {
                cover: if self.cover == Cover::Bikini {
                    Cover::Bikini
                } else {
                    Cover::Full
                },
                cut: Neckline::Closed,
                lacing: None,
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
                let phase = (angle * count as f32 / std::f32::consts::TAU).fract();
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
}

impl Layer {
    /// A petticoat a little longer than the skirt, evenly hemmed.
    pub const fn petticoat(color: Vec3, sheer: bool) -> Self {
        Self {
            color,
            sheer,
            longer: 0.035,
            hem: Hem::Even,
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
    pub const PLAIN: Skirt = Skirt {
        color: Vec3::ZERO,
        length: 0.0,
        split: 0.0,
        trim: None,
        cutaway: false,
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
    /// A knotted sash with tails hanging at one hip.
    Sash(Vec3),
    /// A low V belt hung with coins.
    Coins(Vec3),
    /// Cords crossing round the waist, tied in front.
    Crossed(Vec3),
}

/// The small things an outfit adds, each fitted to any wearer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Trimmings {
    pub neck: Option<Neck>,
    /// Bands round the upper arms.
    pub armlets: Option<Vec3>,
    pub waist: Option<Waist>,
    /// Stiff pointed shoulders standing up over the arms.
    pub peaks: Option<Vec3>,
    /// A cloth tie knotted round one thigh.
    pub garter: Option<Vec3>,
}

impl Trimmings {
    pub const NONE: Trimmings = Trimmings {
        neck: None,
        armlets: None,
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

/// One humanoid's clothes and armour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Outfit {
    pub head: Headwear,
    pub tunic: Vec3,
    /// The tunic's front: its opening, edging, lacing and support.
    pub bodice: Bodice,
    /// Binding or gorget pigment selected with the garment.
    pub collar_edge: Vec3,
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
    pub hands: Vec3,
    pub legs: Legs,
    pub feet: Feet,
    pub outer: Option<Outer>,
    /// The tunic's free lower panels.
    pub skirt: Option<Skirt>,
    /// A cloak and its lining.
    pub cloak: Option<(Vec3, Vec3)>,
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
    /// A strapless sweetheart bodice, sheer puffed sleeves, a full skirt over
    /// petticoats, a choker and sparkling heels.
    Princess,
    /// A laced black bodice over red, belled sleeves, a broad belt and a
    /// sash, a ragged red skirt, a tricorn and cuffed boots.
    Pirate,
    /// A plunging halter over a bare midriff, a belt and shoulder strap, a
    /// long slit skirt, bracers and gladiator sandals.
    Maiden,
    /// A laced corset bodice, sheer puffed sleeves, a tiered skirt, a choker,
    /// a witch's hat and heeled sandals.
    Witch,
}

impl Wardrobe {
    pub const ALL: [Wardrobe; 18] = [
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
        Wardrobe::Pirate,
        Wardrobe::Maiden,
        Wardrobe::Witch,
    ];

    pub fn parse(name: &str) -> Option<Wardrobe> {
        Self::ALL
            .into_iter()
            .find(|w| format!("{w:?}").eq_ignore_ascii_case(name))
    }

    /// The next outfit along the rail, wrapping round.
    pub fn next(self) -> Wardrobe {
        let at = Self::ALL.iter().position(|&w| w == self).unwrap_or(0);
        Self::ALL[(at + 1) % Self::ALL.len()]
    }
}

/// Whether a look wears separable clothes at all: carved stone, shaggy fur
/// and robed casters are each one garment with the body.
pub(crate) fn dresses(l: &Look) -> bool {
    l.plan == Plan::Biped && !matches!(l.style, Style::Guardian | Style::Yeti | Style::Caster)
}

impl Outfit {
    /// What a look wears: its own kind's outfit unless it was dressed in
    /// another.
    pub fn worn(l: &Look) -> Outfit {
        l.outfit.unwrap_or_else(|| Outfit::native(l))
    }

    /// An outfit from the wardrobe, cut for this wearer.
    pub fn from_wardrobe(w: Wardrobe, l: &Look) -> Outfit {
        let fem = l.v.woman;
        let plain_boots = Boots {
            color: LEATHER * 0.75,
            tall: false,
            heel: false,
            buckles: false,
            cuffed: true,
        };
        match w {
            Wardrobe::Native => Outfit::native(l),
            Wardrobe::Commoner => {
                let tunic = Vec3::new(0.52, 0.45, 0.34);
                // Each wearer's own tunic: a laced split V or a deep wrap.
                let wrapped = l.v.pattern % 2 == 1;
                Outfit {
                    head: Headwear::Bare,
                    tunic,
                    bodice: Bodice {
                        cover: Cover::Full,
                        cut: Neckline::fitted(
                            fem,
                            if wrapped {
                                Neckline::WRAP
                            } else {
                                Neckline::SPLIT
                            },
                        ),
                        edge: tunic * 0.72,
                        lacing: (!wrapped).then_some(LEATHER * 0.55),
                        support: Support::Loose,
                        ..Bodice::PLAIN
                    },
                    collar_edge: tunic * 0.72,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: None,
                    belt: true,
                    trimmings: Trimmings::NONE,
                    baldric: None,
                    arms: Arms::Sleeves(tunic),
                    cuffs: None,
                    hands: l.skin,
                    legs: Legs::Trousers(Vec3::new(0.3, 0.24, 0.17)),
                    feet: Feet::Boots(plain_boots),
                    outer: None,
                    skirt: Some(Skirt {
                        color: tunic * 0.98,
                        length: if fem { 0.24 } else { 0.22 },
                        split: if fem { 0.055 } else { 0.0 },
                        trim: None,
                        cutaway: false,
                        ..Skirt::PLAIN
                    }),
                    cloak: None,
                }
            }
            Wardrobe::Ranger => {
                let tunic = Vec3::new(0.27, 0.4, 0.24);
                Outfit {
                    head: Headwear::HoodDown(Vec3::new(0.2, 0.28, 0.17)),
                    tunic,
                    bodice: Bodice {
                        cover: Cover::Full,
                        cut: Neckline::fitted(fem, Neckline::DEEP),
                        edge: tunic * 0.72,
                        lacing: None,
                        support: Support::Loose,
                        ..Bodice::PLAIN
                    },
                    collar_edge: tunic * 0.72,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: Some(Pouch { color: LEATHER }),
                    belt: true,
                    trimmings: Trimmings::NONE,
                    baldric: Some(Baldric {
                        color: LEATHER,
                        reverse: false,
                        buckle: true,
                    }),
                    arms: Arms::Sleeves(tunic),
                    cuffs: Some(LEATHER),
                    hands: LEATHER,
                    legs: Legs::Trousers(Vec3::new(0.22, 0.2, 0.15)),
                    feet: Feet::Boots(Boots {
                        color: LEATHER * 0.7,
                        tall: true,
                        heel: fem,
                        buckles: true,
                        cuffed: true,
                    }),
                    // A leather jerkin, or an open padded gambeson, by the
                    // wearer's own roll.
                    outer: Some({
                        let gambeson = l.v.pattern / 2 % 2 == 1;
                        let shell = if gambeson {
                            Vec3::new(0.46, 0.4, 0.3)
                        } else {
                            LEATHER * 0.85
                        };
                        Outer::Shell {
                            color: shell,
                            top: 0.465,
                            sleeveless: true,
                            leather: !gambeson,
                            quilted: gambeson,
                            underbust: false,
                            peplum: Some(Skirt {
                                color: shell * 0.9,
                                length: 0.08,
                                split: 0.06,
                                trim: Some(shell * 0.9 * 0.7),
                                cutaway: false,
                                ..Skirt::PLAIN
                            }),
                        }
                    }),
                    skirt: Some(Skirt {
                        color: tunic * 0.98,
                        length: if fem { 0.19 } else { 0.22 },
                        split: 0.055,
                        trim: None,
                        cutaway: false,
                        ..Skirt::PLAIN
                    }),
                    cloak: Some((Vec3::new(0.2, 0.28, 0.17), Vec3::new(0.5, 0.42, 0.28))),
                }
            }
            Wardrobe::Knight => Outfit {
                head: Headwear::Helm,
                tunic: l.cloth,
                // Her cuirass is the fantasy low-cut plate: one shell with
                // its upper edge lowered in a V, less cover by choice.
                bodice: Bodice {
                    cover: Cover::Full,
                    cut: Neckline::chosen(fem, l.v.pattern / 2, &[Neckline::DEEP, Neckline::COURT]),
                    edge: l.cloth * 0.72,
                    lacing: None,
                    support: Support::Structured,
                    ..Bodice::PLAIN
                },
                collar_edge: GILT,
                armhole_edge: None,
                torso_ornament: None,
                pouch: None,
                belt: true,
                trimmings: Trimmings::NONE,
                baldric: Some(Baldric {
                    color: LEATHER,
                    reverse: true,
                    buckle: true,
                }),
                arms: Arms::Plate,
                cuffs: Some(STEEL * 0.85),
                hands: STEEL,
                legs: Legs::Plate,
                feet: Feet::Boots(Boots {
                    color: STEEL,
                    tall: false,
                    heel: false,
                    buckles: false,
                    cuffed: true,
                }),
                outer: Some(Outer::Cuirass),
                skirt: Some(Skirt {
                    color: CREAM * 0.86,
                    length: 0.26,
                    split: 0.0,
                    trim: None,
                    cutaway: false,
                    ..Skirt::PLAIN
                }),
                cloak: None,
            },
            Wardrobe::Courtier => {
                let gown = l.cloth * 0.9 + Vec3::new(0.05, 0.0, 0.06);
                Outfit {
                    head: Headwear::Bare,
                    tunic: gown,
                    bodice: Bodice {
                        cover: Cover::Full,
                        cut: Neckline::chosen(
                            fem,
                            l.v.pattern / 2,
                            &[Neckline::COURT, Neckline::WRAP],
                        ),
                        edge: gown * 0.72,
                        lacing: None,
                        support: Support::Structured,
                        ..Bodice::PLAIN
                    },
                    collar_edge: l.trim,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: None,
                    belt: true,
                    trimmings: Trimmings::NONE,
                    baldric: None,
                    arms: Arms::Sleeves(gown),
                    cuffs: Some(l.trim),
                    hands: l.skin,
                    legs: Legs::Fitted(DARK * 2.0),
                    feet: Feet::Boots(Boots {
                        color: DARK * 1.6,
                        tall: false,
                        heel: fem,
                        buckles: false,
                        cuffed: false,
                    }),
                    outer: None,
                    skirt: Some(Skirt {
                        color: gown,
                        length: if fem { 0.72 } else { 0.6 },
                        split: 0.05,
                        trim: Some(l.trim),
                        cutaway: false,
                        ..Skirt::PLAIN
                    }),
                    cloak: Some((gown * 0.55, l.trim)),
                }
            }
            Wardrobe::Leotard => {
                let knit = l.cloth * 0.8 + Vec3::new(0.08, 0.02, 0.1);
                Outfit {
                    head: Headwear::Bare,
                    tunic: knit,
                    bodice: Bodice {
                        cover: Cover::Full,
                        cut: Neckline::fitted(
                            fem,
                            Neckline::Plunge {
                                depth: 0.5,
                                width: 0.17,
                                wrap: false,
                            },
                        ),
                        edge: knit * 0.8,
                        lacing: None,
                        support: Support::None,
                        ..Bodice::PLAIN
                    },
                    collar_edge: knit * 0.8,
                    armhole_edge: Some(knit * 0.8),
                    torso_ornament: None,
                    pouch: None,
                    belt: false,
                    trimmings: Trimmings::NONE,
                    baldric: None,
                    arms: Arms::Bare,
                    cuffs: None,
                    hands: l.skin,
                    legs: Legs::Bare(l.skin),
                    feet: Feet::Bare(l.skin),
                    outer: None,
                    skirt: None,
                    cloak: None,
                }
            }
            Wardrobe::Sarong => {
                // A bright print, the sarong a shade deeper than the top.
                let print = l.cloth * 0.6 + Vec3::new(0.3, 0.14, 0.06);
                Outfit {
                    head: Headwear::Bare,
                    tunic: print,
                    bodice: Bodice {
                        cover: Cover::Bikini,
                        cut: Neckline::Closed,
                        edge: print * 0.75,
                        lacing: None,
                        support: Support::None,
                        ..Bodice::PLAIN
                    },
                    collar_edge: print * 0.75,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: None,
                    belt: false,
                    trimmings: Trimmings::NONE,
                    baldric: None,
                    arms: Arms::Bare,
                    cuffs: None,
                    hands: l.skin,
                    legs: Legs::Bare(l.skin),
                    feet: Feet::Bare(l.skin),
                    outer: None,
                    skirt: Some(Skirt {
                        color: print * 0.85,
                        length: 0.5,
                        split: 0.16,
                        trim: Some(print * 0.65),
                        cutaway: false,
                        ..Skirt::PLAIN
                    }),
                    cloak: None,
                }
            }
            Wardrobe::Tavern => {
                let chemise = CREAM * 0.95;
                let skirt = Vec3::new(0.46, 0.14, 0.12);
                Outfit {
                    head: Headwear::Bare,
                    tunic: chemise,
                    bodice: Bodice {
                        cover: Cover::Full,
                        // A wide, low neckline, the corset lifting beneath it.
                        cut: Neckline::fitted(
                            fem,
                            Neckline::Plunge {
                                depth: 0.6,
                                width: 0.2,
                                wrap: false,
                            },
                        ),
                        edge: chemise * 0.82,
                        lacing: None,
                        support: Support::Structured,
                        ..Bodice::PLAIN
                    },
                    collar_edge: chemise * 0.82,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: Some(Pouch { color: LEATHER }),
                    belt: false,
                    trimmings: Trimmings::NONE,
                    baldric: None,
                    arms: Arms::Sleeves(chemise),
                    cuffs: Some(chemise * 0.85),
                    hands: l.skin,
                    legs: Legs::Fitted(DARK * 2.2),
                    feet: Feet::Boots(Boots {
                        color: LEATHER * 0.7,
                        tall: false,
                        heel: fem,
                        buckles: false,
                        cuffed: true,
                    }),
                    outer: Some(Outer::Shell {
                        color: LEATHER * 0.6,
                        top: 0.32,
                        sleeveless: true,
                        leather: true,
                        quilted: false,
                        underbust: true,
                        peplum: None,
                    }),
                    skirt: Some(Skirt {
                        color: skirt,
                        length: if fem { 0.62 } else { 0.5 },
                        split: 0.0,
                        trim: Some(skirt * 0.6),
                        cutaway: false,
                        ..Skirt::PLAIN
                    }),
                    cloak: None,
                }
            }
            Wardrobe::Queen => {
                let black = Vec3::new(0.09, 0.08, 0.09);
                let red = Vec3::new(0.66, 0.07, 0.08);
                Outfit {
                    tunic: black,
                    bodice: Bodice {
                        cut: Neckline::Plunge {
                            depth: 0.3,
                            width: 0.07,
                            wrap: false,
                        },
                        edge: red,
                        support: Support::Structured,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    collar_edge: red,
                    armhole_edge: Some(red),
                    trimmings: Trimmings {
                        neck: Some(Neck::Stand(black)),
                        peaks: Some(black),
                        ..Trimmings::NONE
                    },
                    baldric: Some(Baldric {
                        color: STEEL * 0.55,
                        reverse: false,
                        buckle: false,
                    }),
                    outer: Some(Outer::Shell {
                        color: black,
                        top: 0.465,
                        sleeveless: true,
                        leather: true,
                        quilted: false,
                        underbust: false,
                        peplum: Some(Skirt {
                            color: black,
                            length: 0.09,
                            trim: Some(red),
                            hem: Hem::Points {
                                count: 6,
                                depth: 0.7,
                            },
                            ..Skirt::PLAIN
                        }),
                    }),
                    skirt: Some(Skirt {
                        color: black,
                        length: 0.34,
                        split: 0.17,
                        trim: Some(red),
                        hem: Hem::Points {
                            count: 6,
                            depth: 0.25,
                        },
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Boots(Boots {
                        color: black * 1.4,
                        tall: true,
                        heel: fem,
                        buckles: true,
                        cuffed: true,
                    }),
                    cloak: Some((red, red * 0.75)),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Pharaoh => {
                let gold = Vec3::new(0.84, 0.64, 0.28);
                let night = Vec3::new(0.08, 0.07, 0.08);
                Outfit {
                    tunic: gold,
                    bodice: Bodice {
                        cut: Neckline::Keyhole {
                            depth: 0.72,
                            width: 0.13,
                        },
                        edge: gold * 0.8,
                        support: Support::Structured,
                        straps: Straps::Halter,
                        pattern: Pattern::Ribbed,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    collar_edge: gold,
                    arms: Arms::Styled(Sleeve {
                        color: gold,
                        cut: SleeveCut::Gauntlet,
                        puff: false,
                        sheer: false,
                        flare: Some(night * 2.0),
                    }),
                    trimmings: Trimmings {
                        neck: Some(Neck::Collar {
                            metal: GILT,
                            beads: night * 1.5,
                        }),
                        ..Trimmings::NONE
                    },
                    head: Headwear::Circlet {
                        color: GILT,
                        gem: Some(Vec3::new(0.12, 0.22, 0.75)),
                        broad: true,
                    },
                    skirt: Some(Skirt {
                        color: night,
                        length: 0.82,
                        fullness: 0.85,
                        panel: Some((gold, 0.2)),
                        pattern: Pattern::Ribbed,
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: GILT,
                        heel: fem,
                        sandal: true,
                        straps: 0.0,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Goddess => {
                let white = Vec3::new(0.95, 0.94, 0.92);
                Outfit {
                    tunic: white,
                    bodice: Bodice {
                        cut: Neckline::Plunge {
                            depth: 0.8,
                            width: 0.17,
                            wrap: false,
                        },
                        edge: GILT,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    collar_edge: GILT,
                    armhole_edge: Some(GILT),
                    trimmings: Trimmings {
                        armlets: Some(GILT),
                        waist: Some(Waist::Crossed(GILT)),
                        ..Trimmings::NONE
                    },
                    head: Headwear::Wreath(GILT),
                    skirt: Some(Skirt {
                        color: white,
                        length: 0.24,
                        trim: Some(GILT),
                        layers: [
                            Some(Layer {
                                color: white,
                                sheer: true,
                                longer: 0.45,
                                hem: Hem::HighLow(0.55),
                            }),
                            None,
                        ],
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: GILT,
                        heel: fem,
                        sandal: true,
                        straps: 1.0,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Juliet => {
                let navy = Vec3::new(0.12, 0.13, 0.4);
                let pale = Vec3::new(0.8, 0.86, 0.96);
                Outfit {
                    tunic: pale,
                    bodice: Bodice {
                        cut: Neckline::Plunge {
                            depth: 0.45,
                            width: 0.17,
                            wrap: false,
                        },
                        edge: GILT,
                        lacing: Some(DARK * 3.0),
                        support: Support::Structured,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    collar_edge: GILT,
                    arms: Arms::Styled(Sleeve {
                        color: navy,
                        cut: SleeveCut::Bell,
                        puff: true,
                        sheer: false,
                        flare: Some(navy * 0.85),
                    }),
                    outer: Some(Outer::Shell {
                        color: navy,
                        top: 0.465,
                        sleeveless: true,
                        leather: false,
                        quilted: false,
                        underbust: false,
                        peplum: None,
                    }),
                    head: Headwear::Veiled {
                        cap: navy,
                        veil: Vec3::new(0.95, 0.95, 0.97),
                    },
                    skirt: Some(Skirt {
                        color: navy,
                        length: 0.8,
                        fullness: 1.15,
                        panel: Some((pale, 0.2)),
                        trim: Some(GILT),
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: GILT,
                        heel: fem,
                        sandal: false,
                        straps: 0.0,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Jungle => {
                let hide = Vec3::new(0.8, 0.6, 0.32);
                let spots = Pattern::Leopard(Vec3::new(0.28, 0.16, 0.09));
                Outfit {
                    tunic: hide,
                    bodice: Bodice {
                        cover: Cover::Crop(Hem::Points {
                            count: 14,
                            depth: 0.7,
                        }),
                        straps: Straps::OneShoulder,
                        edge: LEATHER * 0.8,
                        support: Support::None,
                        pattern: spots,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    trimmings: Trimmings {
                        neck: Some(Neck::Teeth { cord: DARK * 2.0 }),
                        armlets: Some(DARK * 2.0),
                        garter: Some(hide),
                        ..Trimmings::NONE
                    },
                    skirt: Some(Skirt {
                        color: hide,
                        length: 0.24,
                        split: 0.07,
                        trim: Some(LEATHER * 0.8),
                        hem: Hem::Points {
                            count: 7,
                            depth: 0.45,
                        },
                        pattern: spots,
                        ..Skirt::PLAIN
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Genie => {
                let silk = Vec3::new(0.14, 0.58, 0.86);
                Outfit {
                    tunic: silk,
                    bodice: Bodice {
                        cover: Cover::Bikini,
                        edge: GILT,
                        support: Support::None,
                        ..Bodice::PLAIN
                    },
                    arms: Arms::Styled(Sleeve {
                        color: silk,
                        cut: SleeveCut::OffShoulder,
                        puff: false,
                        sheer: true,
                        flare: None,
                    }),
                    cuffs: Some(GILT),
                    trimmings: Trimmings {
                        neck: Some(Neck::Jewel {
                            band: silk * 0.8,
                            gem: GILT,
                        }),
                        waist: Some(Waist::Coins(GILT)),
                        ..Trimmings::NONE
                    },
                    head: Headwear::Circlet {
                        color: GILT,
                        gem: Some(silk),
                        broad: false,
                    },
                    skirt: Some(Skirt {
                        color: silk,
                        length: 0.72,
                        split: 0.22,
                        fullness: 1.1,
                        sheer: true,
                        hem: Hem::HighLow(0.15),
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: GILT,
                        heel: fem,
                        sandal: true,
                        straps: 0.15,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Princess => {
                let sky = Vec3::new(0.62, 0.78, 0.96);
                let frost = Vec3::new(0.86, 0.9, 0.98);
                Outfit {
                    tunic: sky,
                    bodice: Bodice {
                        straps: Straps::Strapless,
                        edge: frost,
                        support: Support::Structured,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    arms: Arms::Styled(Sleeve {
                        color: frost,
                        cut: SleeveCut::Puff,
                        puff: true,
                        sheer: true,
                        flare: Some(sky),
                    }),
                    trimmings: Trimmings {
                        neck: Some(Neck::Choker(DARK * 1.5)),
                        ..Trimmings::NONE
                    },
                    skirt: Some(Skirt {
                        color: sky,
                        length: 0.3,
                        fullness: 1.7,
                        layers: [
                            Some(Layer::petticoat(frost, true)),
                            Some(Layer::petticoat(frost, false)),
                        ],
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: Vec3::new(0.82, 0.85, 0.92),
                        heel: fem,
                        sandal: false,
                        straps: 0.0,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Pirate => {
                let red = Vec3::new(0.6, 0.08, 0.1);
                let velvet = Vec3::new(0.1, 0.08, 0.1);
                Outfit {
                    tunic: red,
                    bodice: Bodice {
                        cut: Neckline::Plunge {
                            depth: 0.7,
                            width: 0.14,
                            wrap: false,
                        },
                        edge: GILT,
                        lacing: Some(DARK * 2.0),
                        support: Support::Structured,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    collar_edge: GILT,
                    arms: Arms::Styled(Sleeve {
                        color: velvet,
                        cut: SleeveCut::Bell,
                        puff: false,
                        sheer: false,
                        flare: Some(red),
                    }),
                    outer: Some(Outer::Shell {
                        color: velvet,
                        top: 0.465,
                        sleeveless: true,
                        leather: false,
                        quilted: false,
                        underbust: false,
                        peplum: None,
                    }),
                    belt: true,
                    trimmings: Trimmings {
                        waist: Some(Waist::Sash(Vec3::new(0.16, 0.13, 0.1))),
                        ..Trimmings::NONE
                    },
                    head: Headwear::Tricorn {
                        color: velvet * 1.3,
                        trim: GILT,
                    },
                    skirt: Some(Skirt {
                        color: red,
                        length: 0.46,
                        split: 0.12,
                        hem: Hem::Points {
                            count: 5,
                            depth: 0.45,
                        },
                        layers: [
                            Some(Layer {
                                hem: Hem::Points {
                                    count: 7,
                                    depth: 0.4,
                                },
                                ..Layer::petticoat(red * 0.82, false)
                            }),
                            None,
                        ],
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Boots(Boots {
                        color: velvet * 1.4,
                        tall: true,
                        heel: fem,
                        buckles: true,
                        cuffed: true,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Maiden => {
                let teal = Vec3::new(0.15, 0.46, 0.5);
                Outfit {
                    tunic: teal,
                    bodice: Bodice {
                        cover: Cover::Crop(Hem::Even),
                        cut: Neckline::Plunge {
                            depth: 0.88,
                            width: 0.15,
                            wrap: false,
                        },
                        edge: teal * 0.8,
                        straps: Straps::Halter,
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    belt: true,
                    baldric: Some(Baldric {
                        color: LEATHER * 0.7,
                        reverse: false,
                        buckle: true,
                    }),
                    cuffs: Some(LEATHER * 0.6),
                    skirt: Some(Skirt {
                        color: teal,
                        length: 0.8,
                        split: 0.24,
                        fullness: 1.15,
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: LEATHER,
                        heel: fem,
                        sandal: true,
                        straps: 1.0,
                    }),
                    ..Outfit::base(l)
                }
            }
            Wardrobe::Witch => {
                let violet = Vec3::new(0.42, 0.15, 0.6);
                let black = DARK * 1.6;
                Outfit {
                    tunic: violet,
                    bodice: Bodice {
                        cut: Neckline::Plunge {
                            depth: 0.75,
                            width: 0.14,
                            wrap: false,
                        },
                        edge: black,
                        lacing: Some(black),
                        support: Support::Structured,
                        pattern: Pattern::Dots(Vec3::new(0.85, 0.75, 0.95)),
                        ..Bodice::PLAIN
                    }
                    .fitted(fem),
                    collar_edge: black,
                    arms: Arms::Styled(Sleeve {
                        color: Vec3::new(0.62, 0.36, 0.86),
                        cut: SleeveCut::Puff,
                        puff: true,
                        sheer: true,
                        flare: Some(black),
                    }),
                    outer: Some(Outer::Shell {
                        color: black,
                        top: 0.465,
                        sleeveless: true,
                        leather: true,
                        quilted: false,
                        underbust: true,
                        peplum: None,
                    }),
                    belt: true,
                    trimmings: Trimmings {
                        neck: Some(Neck::Choker(black)),
                        ..Trimmings::NONE
                    },
                    head: Headwear::WitchHat {
                        color: black,
                        band: Vec3::new(0.95, 0.66, 0.8),
                    },
                    skirt: Some(Skirt {
                        color: violet,
                        length: 0.28,
                        fullness: 1.6,
                        pattern: Pattern::Dots(Vec3::new(0.85, 0.75, 0.95)),
                        layers: [
                            Some(Layer::petticoat(black, false)),
                            Some(Layer {
                                longer: 0.075,
                                hem: Hem::Points {
                                    count: 14,
                                    depth: 0.25,
                                },
                                ..Layer::petticoat(violet, true)
                            }),
                        ],
                        ..Skirt::PLAIN
                    }),
                    feet: Feet::Shoes(Shoes {
                        color: black,
                        heel: fem,
                        sandal: true,
                        straps: 0.0,
                    }),
                    ..Outfit::base(l)
                }
            }
        }
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
            armhole_edge: None,
            torso_ornament: None,
            pouch: None,
            belt: false,
            trimmings: Trimmings::NONE,
            baldric: None,
            arms: Arms::Bare,
            cuffs: None,
            hands: l.skin,
            legs: Legs::Bare(l.skin),
            feet: Feet::Bare(l.skin),
            outer: None,
            skirt: None,
            cloak: None,
        }
    }

    /// Each kind's own outfit, as drawn on its concept sheet.
    pub fn native(l: &Look) -> Outfit {
        let style = l.style;
        let fem = l.v.woman;
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
        // Bare-limbed kinds show skin where others wear sleeves.
        let arms = match style {
            Style::Sentinel => Arms::Plate,
            Style::Hobgoblin
            | Style::Zombie
            | Style::Orc
            | Style::Troll
            | Style::Nymph
            | Style::Medusa
            | Style::Phantom => Arms::Bare,
            Style::Guardian => Arms::Sleeves(l.skin),
            Style::Yeti | Style::Leprechaun => Arms::Sleeves(l.cloth),
            Style::Vampire => Arms::Sleeves(Vec3::new(0.22, 0.12, 0.17)),
            _ => Arms::Sleeves(tunic),
        };
        // Cloth trousers stand off the leg; skin, stockings and mail stay fitted.
        let legs = match style {
            Style::Sentinel => Legs::Plate,
            Style::Hero
            | Style::Scout
            | Style::Common
            | Style::Vampire
            | Style::Hobgoblin
            | Style::Zombie
            | Style::Orc
            | Style::Leprechaun
            | Style::Phantom
            | Style::Wraith => Legs::Trousers(pants),
            _ => Legs::Fitted(pants),
        };
        let feet = match style {
            Style::Troll => Feet::Bare(l.skin * 0.88),
            Style::Nymph => Feet::Bare(l.skin),
            _ => Feet::Boots(Boots {
                color: match style {
                    Style::Guardian => l.skin * 0.85,
                    Style::Yeti => l.skin,
                    Style::Sentinel => STEEL,
                    Style::Zombie => l.skin * 0.8,
                    Style::Vampire => DARK,
                    Style::Leprechaun => LEATHER * 0.9,
                    Style::Medusa => Vec3::new(0.4, 0.24, 0.14),
                    Style::Wraith => Vec3::new(0.2, 0.16, 0.14),
                    _ => LEATHER * 0.75,
                },
                tall: fem && matches!(style, Style::Hero | Style::Scout),
                heel: geometry::heeled_boots(style, fem),
                buckles: matches!(
                    style,
                    Style::Hero | Style::Scout | Style::Hobgoblin | Style::Orc | Style::Leprechaun
                ),
                cuffed: !matches!(style, Style::Zombie | Style::Yeti),
            }),
        };
        let hands = match style {
            Style::Yeti => l.skin,
            Style::Sentinel => STEEL,
            Style::Hero | Style::Scout => LEATHER,
            _ => l.skin,
        };
        let cuffs = match style {
            Style::Hero | Style::Scout | Style::Hobgoblin => Some(LEATHER),
            Style::Orc | Style::Troll | Style::Nymph | Style::Phantom => Some(LEATHER * 0.85),
            Style::Leprechaun => Some(CREAM),
            Style::Medusa => Some(l.trim),
            Style::Sentinel => Some(STEEL * 0.85),
            Style::Caster | Style::Vampire | Style::Wraith => Some(l.trim),
            _ => None,
        };
        // A fitted outer garment: a jerkin, vest or coat over the tunic.
        let peplum = match style {
            Style::Hero | Style::Scout => Some((0.08, 0.06)),
            Style::Hobgoblin | Style::Orc => Some((0.06, 0.0)),
            _ => None,
        }
        .map(|(length, split)| {
            let color = LEATHER * if style == Style::Scout { 0.85 } else { 0.9 };
            Skirt {
                color,
                length,
                split,
                trim: Some(color * 0.7),
                cutaway: false,
                ..Skirt::PLAIN
            }
        });
        let shell = |color: Vec3, top: f32, sleeveless: bool, leather: bool| {
            Some(Outer::Shell {
                color,
                top,
                sleeveless,
                leather,
                quilted: false,
                underbust: false,
                peplum,
            })
        };
        let outer = match style {
            Style::Hero => shell(LEATHER, 0.465, true, true),
            Style::Scout => shell(LEATHER * 0.85, 0.465, true, true),
            Style::Hobgoblin => shell(LEATHER * 0.9, 0.43, true, true),
            Style::Vampire => shell(Vec3::new(0.32, 0.05, 0.07), 0.47, false, false),
            Style::Orc => shell(LEATHER * 0.95, 0.44, true, true),
            Style::Leprechaun => shell(l.cloth, 0.47, false, false),
            Style::Wraith => shell(l.cloth * 1.15, 0.48, false, false),
            Style::Sentinel => Some(Outer::Cuirass),
            _ => None,
        };
        let skirt = match style {
            Style::Guardian | Style::Yeti | Style::Caster => None,
            Style::Sentinel => Some((CREAM * 0.86, 0.26)),
            Style::Zombie => Some((tunic * 0.8, 0.2)),
            Style::Vampire => Some((DARK, 0.3)),
            Style::Orc => Some((LEATHER * 0.8, 0.18)),
            Style::Troll => Some((l.cloth, 0.26)),
            Style::Leprechaun => Some((l.cloth, 0.4)),
            Style::Nymph => Some((l.cloth, 0.2)),
            Style::Medusa => Some((l.cloth, 0.62)),
            Style::Phantom => Some((l.cloth * 0.9, 0.5)),
            Style::Wraith => Some((l.cloth, 0.66)),
            Style::Hero | Style::Scout if fem => Some((tunic * 0.98, 0.19)),
            _ => Some((tunic * 0.98, 0.22)),
        };
        let skirt = skirt.map(|(color, length)| Skirt {
            color,
            length,
            split: match style {
                Style::Hero | Style::Scout => 0.055,
                Style::Leprechaun => 0.07,
                Style::Medusa => 0.05,
                _ => 0.0,
            },
            trim: matches!(style, Style::Medusa | Style::Leprechaun).then_some(l.trim),
            cutaway: geometry::cutaway_coat(style),
            ..Skirt::PLAIN
        });
        let cloak = match style {
            Style::Hero => Some((l.cloth * 0.62, l.trim)),
            Style::Vampire => Some((DARK, l.trim)),
            _ => None,
        };
        // Scouts and casters wear their hoods up unless pushed back; a
        // wraith's is always up. Hoods sometimes end in a long soft point.
        let hood_down = matches!(style, Style::Caster | Style::Scout) && l.v.pattern >= 2;
        let hood = match style {
            Style::Caster => l.cloth,
            Style::Wraith => l.cloth * 0.85,
            _ => Vec3::new(0.3, 0.42, 0.22),
        };
        let head = match style {
            Style::Sentinel => Headwear::Helm,
            Style::Caster | Style::Scout if hood_down => Headwear::HoodDown(hood),
            Style::Caster | Style::Scout | Style::Wraith => Headwear::Hood {
                color: hood,
                pointed: style == Style::Scout || l.aura == Some(Accent::Ember),
            },
            _ => Headwear::Bare,
        };
        let neckline_edge = if style == Style::Medusa {
            l.trim
        } else {
            tunic * 0.72
        };
        let torso_ornament = match style {
            Style::Orc | Style::Troll => Some(TorsoOrnament::Harness {
                strap: LEATHER * 0.62,
                ring: Vec3::new(0.82, 0.58, 0.26),
                shoulder_straps: (style == Style::Troll).then_some(LEATHER * 0.7),
            }),
            Style::Leprechaun => Some(TorsoOrnament::StockAndButtons {
                stock: CREAM,
                buttons: GILT,
                belt: LEATHER * 0.6,
            }),
            Style::Nymph => Some(TorsoOrnament::LeafBodice {
                leaves: l.cloth,
                pin: l.trim,
            }),
            Style::Medusa => Some(TorsoOrnament::NeckBandAndDisc { metal: l.trim }),
            Style::Phantom | Style::Wraith => Some(TorsoOrnament::KnottedSash {
                sash: if style == Style::Wraith {
                    l.trim
                } else {
                    LEATHER * 0.7
                },
                scarf: (style == Style::Wraith).then_some(l.trim * 0.9),
            }),
            _ => None,
        };
        Outfit {
            head,
            tunic,
            bodice: Bodice {
                cover: Cover::Full,
                cut: Neckline::fitted(
                    fem && !matches!(
                        style,
                        Style::Sentinel
                            | Style::Guardian
                            | Style::Yeti
                            | Style::Hobgoblin
                            | Style::Zombie
                    ),
                    // Each kind's sheet sets how low her own garment plunges;
                    // a mage's robe is the open-front court plunge.
                    match style {
                        Style::Vampire | Style::Nymph | Style::Caster => Neckline::COURT,
                        // The ghosts' shrouds cross over, knotted at the waist.
                        Style::Phantom | Style::Wraith => Neckline::WRAP,
                        Style::Leprechaun | Style::Troll => Neckline::SPLIT,
                        _ => Neckline::DEEP,
                    },
                ),
                edge: neckline_edge,
                lacing: matches!(style, Style::Hero | Style::Scout).then_some(LEATHER * 0.6),
                support: match style {
                    Style::Orc
                    | Style::Troll
                    | Style::Vampire
                    | Style::Leprechaun
                    | Style::Sentinel
                    | Style::Medusa => Support::Structured,
                    Style::Nymph | Style::Zombie => Support::None,
                    _ => Support::Loose,
                },
                ..Bodice::PLAIN
            },
            collar_edge: l.trim,
            armhole_edge: matches!(style, Style::Medusa | Style::Phantom | Style::Nymph)
                .then_some(neckline_edge),
            torso_ornament,
            pouch: matches!(style, Style::Hero | Style::Scout).then_some(Pouch { color: LEATHER }),
            belt: true,
            trimmings: Trimmings::NONE,
            baldric: match style {
                Style::Hero => Some(Baldric {
                    color: LEATHER * 0.7,
                    reverse: true,
                    buckle: true,
                }),
                Style::Scout => Some(Baldric {
                    color: LEATHER,
                    reverse: false,
                    buckle: false,
                }),
                _ => None,
            },
            arms,
            cuffs,
            hands,
            legs,
            feet,
            outer,
            skirt,
            cloak,
        }
    }

    /// The colour the upper arms show: skin, sleeve or steel.
    pub fn sleeve(&self, skin: Vec3) -> Vec3 {
        match self.arms {
            Arms::Bare => skin,
            Arms::Sleeves(color) => color,
            Arms::Plate => STEEL,
            Arms::Styled(sleeve) => match sleeve.cut {
                SleeveCut::Bell => sleeve.color,
                SleeveCut::Puff | SleeveCut::Gauntlet | SleeveCut::OffShoulder => skin,
            },
        }
    }

    /// The colour the forearms show: skin, sleeve, gauntlet or steel.
    pub fn forearm(&self, skin: Vec3) -> Vec3 {
        match self.arms {
            Arms::Styled(sleeve) => match sleeve.cut {
                SleeveCut::Bell | SleeveCut::Gauntlet => sleeve.color,
                SleeveCut::Puff | SleeveCut::OffShoulder => skin,
            },
            _ => self.sleeve(skin),
        }
    }

    /// The colour the legs show: skin, hose, cloth or the mail under plate.
    pub fn pants(&self) -> Vec3 {
        match self.legs {
            Legs::Fitted(color) | Legs::Trousers(color) | Legs::Bare(color) => color,
            Legs::Plate => IRON,
        }
    }

    /// The colour over the seat: the legwear's, or the torso garment's
    /// where it is cut high over bare legs.
    pub fn seat(&self) -> Vec3 {
        match self.legs {
            Legs::Bare(_) => self.tunic,
            _ => self.pants(),
        }
    }

    /// The colour of the boots, or the bare feet.
    pub fn boot(&self) -> Vec3 {
        match self.feet {
            Feet::Bare(color) => color,
            Feet::Boots(boots) => boots.color,
            Feet::Shoes(shoes) => shoes.color,
        }
    }
}
