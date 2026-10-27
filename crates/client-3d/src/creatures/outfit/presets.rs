//! Authored wardrobe presets composed from shared wearer-fitted garment types.
use super::*;

pub(super) fn compose(w: Wardrobe, l: &Look) -> Outfit {
    let fem = l.v.woman;
    let plain_boots = Boots {
        color: LEATHER * 0.75,
        tall: false,
        heel: false,
        buckles: false,
        cuffed: true,
    };
    match w {
        Wardrobe::ElvenRanger
        | Wardrobe::Rogue
        | Wardrobe::Paladin
        | Wardrobe::Barbarian
        | Wardrobe::Valkyrie
        | Wardrobe::Cleric
        | Wardrobe::Druid
        | Wardrobe::Bard
        | Wardrobe::VampireNoble
        | Wardrobe::Necromancer
        | Wardrobe::Desert => studies::compose(w, l),
        Wardrobe::Native => Outfit::native(l),
        Wardrobe::Adventurer => {
            // The base every Rogue armour sheet is drawn over.
            let shirt = Vec3::new(0.29, 0.42, 0.21);
            Outfit {
                tunic: shirt,
                bodice: Bodice {
                    cover: Cover::Full,
                    cut: Neckline::fitted(fem, Neckline::DEEP),
                    edge: shirt * 0.72,
                    lacing: None,
                    support: Support::Loose,
                    ..Bodice::PLAIN
                },
                collar_edge: shirt * 0.72,
                belt: true,
                arms: Arms::Styled(Sleeve {
                    color: shirt,
                    cut: SleeveCut::Short,
                    puff: false,
                    puff_lining: None,
                    sheer: false,
                    flare: None,
                }),
                legs: Legs::Trousers(Vec3::new(0.3, 0.2, 0.12)),
                feet: Feet::Boots(Boots {
                    color: LEATHER * 1.05,
                    tall: true,
                    heel: false,
                    buckles: false,
                    cuffed: true,
                }),
                // Split tails hang below the belt, front and back.
                skirt: Some(Skirt {
                    color: shirt * 0.98,
                    length: if fem { 0.2 } else { 0.22 },
                    split: 0.075,
                    trim: None,
                    cutaway: false,
                    ..Skirt::PLAIN
                }),
                ..Outfit::base(l)
            }
        }
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                armhole_edge: None,
                torso_ornament: None,
                pouch: None,
                belt: true,
                trimmings: Trimmings::NONE,
                baldric: None,
                arms: Arms::Sleeves(tunic),
                cuffs: None,
                hands: Pigment::Skin(1.0),
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
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
                hands: Pigment::Fixed(LEATHER),
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
                        opening: ShellOpening::ShapedNeck,
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
                cloak: Some(Cloak::new(
                    Vec3::new(0.2, 0.28, 0.17),
                    Vec3::new(0.5, 0.42, 0.28),
                )),
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
            robe: None,
            details: GarmentDetails::default(),
            neck_drape: None,
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
            hands: Pigment::Fixed(STEEL),
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
                    cut: Neckline::chosen(fem, l.v.pattern / 2, &[Neckline::COURT, Neckline::WRAP]),
                    edge: gown * 0.72,
                    lacing: None,
                    support: Support::Structured,
                    ..Bodice::PLAIN
                },
                collar_edge: l.trim,
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                armhole_edge: None,
                torso_ornament: None,
                pouch: None,
                belt: true,
                trimmings: Trimmings::NONE,
                baldric: None,
                arms: Arms::Sleeves(gown),
                cuffs: Some(l.trim),
                hands: Pigment::Skin(1.0),
                legs: Legs::Fitted(Pigment::Fixed(DARK * 2.0)),
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
                cloak: Some(Cloak::new(gown * 0.55, l.trim)),
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                armhole_edge: Some(knit * 0.8),
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                armhole_edge: None,
                torso_ornament: None,
                pouch: Some(Pouch { color: LEATHER }),
                belt: false,
                trimmings: Trimmings::NONE,
                baldric: None,
                arms: Arms::Sleeves(chemise),
                cuffs: Some(chemise * 0.85),
                hands: Pigment::Skin(1.0),
                legs: Legs::Fitted(Pigment::Fixed(DARK * 2.2)),
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
                    opening: ShellOpening::Closed,
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
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
                    opening: ShellOpening::ShapedNeck,
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
                cloak: Some(Cloak::new(red, red * 0.75)),
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                arms: Arms::Styled(Sleeve {
                    color: gold,
                    cut: SleeveCut::Gauntlet,
                    puff: false,
                    puff_lining: None,
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
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
                            ..Layer::petticoat(white, true)
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                arms: Arms::Styled(Sleeve {
                    color: navy,
                    cut: SleeveCut::Bell,
                    puff: true,
                    puff_lining: None,
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
                    opening: ShellOpening::ShapedNeck,
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
                    puff_lining: None,
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
                    puff_lining: None,
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
        Wardrobe::RoyalPrincess => royal::princess(l),
        Wardrobe::SylvanDuelist => sylvan::duelist(l),
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                arms: Arms::Styled(Sleeve {
                    color: velvet,
                    cut: SleeveCut::Bell,
                    puff: false,
                    puff_lining: None,
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
                    opening: ShellOpening::ShapedNeck,
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
                robe: None,
                details: GarmentDetails::default(),
                neck_drape: None,
                arms: Arms::Styled(Sleeve {
                    color: Vec3::new(0.62, 0.36, 0.86),
                    cut: SleeveCut::Puff,
                    puff: true,
                    puff_lining: None,
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
                    opening: ShellOpening::Closed,
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
