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
}

/// Footwear, fitted to the wearer's feet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Feet {
    /// Bare feet with wrappings at the ankle, in the skin's colour.
    Bare(Vec3),
    Boots(Boots),
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
        /// Its free hem below the belt, including its cut, pigment and trim.
        peplum: Option<Skirt>,
    },
    /// A plate cuirass with its gorget and gilt-edged tabard panels.
    Cuirass,
}

/// The opening belongs to the garment, independently of the wearer species.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Neckline {
    Closed,
    OpenV,
}

impl Neckline {
    fn fitted(woman: bool) -> Self {
        if woman {
            Self::OpenV
        } else {
            Self::Closed
        }
    }
}

/// A free cloth panel, including the construction that travels with it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Skirt {
    pub color: Vec3,
    pub length: f32,
    pub split: f32,
    pub trim: Option<Vec3>,
    pub cutaway: bool,
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
    pub neckline: Neckline,
    pub neckline_edge: Vec3,
    /// Binding or gorget pigment selected with the garment.
    pub collar_edge: Vec3,
    /// Bound edges of a sleeveless bodice, if the garment has them.
    pub armhole_edge: Option<Vec3>,
    pub torso_ornament: Option<TorsoOrnament>,
    pub pouch: Option<Pouch>,
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
}

impl Wardrobe {
    pub const ALL: [Wardrobe; 5] = [
        Wardrobe::Native,
        Wardrobe::Commoner,
        Wardrobe::Ranger,
        Wardrobe::Knight,
        Wardrobe::Courtier,
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
                Outfit {
                    head: Headwear::Bare,
                    tunic,
                    neckline: Neckline::fitted(fem),
                    neckline_edge: tunic * 0.72,
                    collar_edge: tunic * 0.72,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: None,
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
                    }),
                    cloak: None,
                }
            }
            Wardrobe::Ranger => {
                let tunic = Vec3::new(0.27, 0.4, 0.24);
                Outfit {
                    head: Headwear::HoodDown(Vec3::new(0.2, 0.28, 0.17)),
                    tunic,
                    neckline: Neckline::fitted(fem),
                    neckline_edge: tunic * 0.72,
                    collar_edge: tunic * 0.72,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: Some(Pouch { color: LEATHER }),
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
                    outer: Some(Outer::Shell {
                        color: LEATHER * 0.85,
                        top: 0.465,
                        sleeveless: true,
                        leather: true,
                        peplum: Some(Skirt {
                            color: LEATHER * 0.85 * 0.9,
                            length: 0.08,
                            split: 0.06,
                            trim: Some(LEATHER * 0.85 * 0.9 * 0.7),
                            cutaway: false,
                        }),
                    }),
                    skirt: Some(Skirt {
                        color: tunic * 0.98,
                        length: if fem { 0.19 } else { 0.22 },
                        split: 0.055,
                        trim: None,
                        cutaway: false,
                    }),
                    cloak: Some((Vec3::new(0.2, 0.28, 0.17), Vec3::new(0.5, 0.42, 0.28))),
                }
            }
            Wardrobe::Knight => Outfit {
                head: Headwear::Helm,
                tunic: l.cloth,
                neckline: Neckline::Closed,
                neckline_edge: l.cloth * 0.72,
                collar_edge: GILT,
                armhole_edge: None,
                torso_ornament: None,
                pouch: None,
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
                }),
                cloak: None,
            },
            Wardrobe::Courtier => {
                let gown = l.cloth * 0.9 + Vec3::new(0.05, 0.0, 0.06);
                Outfit {
                    head: Headwear::Bare,
                    tunic: gown,
                    neckline: Neckline::fitted(fem),
                    neckline_edge: gown * 0.72,
                    collar_edge: l.trim,
                    armhole_edge: None,
                    torso_ornament: None,
                    pouch: None,
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
                    }),
                    cloak: Some((gown * 0.55, l.trim)),
                }
            }
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
            }
        });
        let shell = |color: Vec3, top: f32, sleeveless: bool, leather: bool| {
            Some(Outer::Shell {
                color,
                top,
                sleeveless,
                leather,
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
            neckline_edge,
            collar_edge: l.trim,
            armhole_edge: matches!(style, Style::Medusa | Style::Phantom | Style::Nymph)
                .then_some(neckline_edge),
            torso_ornament,
            pouch: matches!(style, Style::Hero | Style::Scout).then_some(Pouch { color: LEATHER }),
            neckline: Neckline::fitted(
                fem && !matches!(
                    style,
                    Style::Sentinel
                        | Style::Guardian
                        | Style::Yeti
                        | Style::Caster
                        | Style::Hobgoblin
                        | Style::Zombie
                ),
            ),
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
        }
    }

    /// The colour the legs show: skin, hose, cloth or the mail under plate.
    pub fn pants(&self) -> Vec3 {
        match self.legs {
            Legs::Fitted(color) | Legs::Trousers(color) => color,
            Legs::Plate => IRON,
        }
    }

    /// The colour of the boots, or the bare feet.
    pub fn boot(&self) -> Vec3 {
        match self.feet {
            Feet::Bare(color) => color,
            Feet::Boots(boots) => boots.color,
        }
    }
}
