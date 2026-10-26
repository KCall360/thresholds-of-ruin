//! Paired character-study costumes made from shared, wearer-fitted garments.
//! These recipes choose cuts and materials; geometry never branches on a study.
use super::*;

const GREEN: Vec3 = Vec3::new(0.14, 0.27, 0.22);
const RED: Vec3 = Vec3::new(0.40, 0.055, 0.085);
const BLUE: Vec3 = Vec3::new(0.065, 0.16, 0.40);
const IVORY: Vec3 = Vec3::new(0.88, 0.81, 0.65);
const VIOLET: Vec3 = Vec3::new(0.27, 0.13, 0.32);
const INK: Vec3 = Vec3::new(0.075, 0.065, 0.075);
const TEAL: Vec3 = Vec3::new(0.07, 0.24, 0.27);

fn embroidered(motif: Motif, thread: Vec3) -> Pattern {
    Pattern::Brocade { thread, motif }
}

fn skirt(color: Vec3, length: f32, split: f32, motif: Motif) -> Skirt {
    Skirt {
        color,
        length,
        split,
        trim: Some(GILT),
        pattern: embroidered(motif, GILT * 0.82),
        ..Skirt::PLAIN
    }
}

fn lining(color: Vec3, longer: f32, hem: Hem) -> Layer {
    Layer {
        longer,
        hem,
        ..Layer::petticoat(color, false)
    }
}

fn sleeve(color: Vec3, flare: Vec3, puff: bool) -> Arms {
    Arms::Styled(Sleeve {
        color,
        cut: SleeveCut::Bell,
        puff,
        puff_lining: None,
        sheer: false,
        flare: Some(flare),
    })
}

pub(super) fn compose(wardrobe: Wardrobe, wearer: &Look) -> Outfit {
    debug_assert!(Wardrobe::STUDIES.contains(&wardrobe));
    let woman = wearer.woman();
    let mut outfit = Outfit {
        tunic: LEATHER * 0.65,
        bodice: Bodice {
            // The adult female study cuts expose the sternum while following
            // the resolved chest. The male shirt has its own smaller opening.
            cut: Neckline::COURT,
            male_cut: Some(Neckline::Plunge {
                depth: 0.55,
                width: 0.18,
                wrap: false,
            }),
            edge: GILT,
            lacing: Some(GILT),
            support: Support::Structured,
            ..Bodice::PLAIN
        }
        .fitted(woman),
        collar_edge: GILT,
        robe: None,
        details: GarmentDetails::default(),
        neck_drape: None,
        armhole_edge: Some(GILT),
        belt: true,
        pouch: Some(Pouch { color: LEATHER }),
        cuffs: Some(LEATHER * 0.7),
        legs: Legs::Trousers(INK * 1.8),
        feet: Feet::Boots(Boots {
            color: LEATHER * 0.65,
            tall: true,
            heel: woman,
            buckles: true,
            cuffed: true,
        }),
        ..Outfit::base(wearer)
    };
    match wardrobe {
        Wardrobe::ElvenRanger => {
            outfit.arms = Arms::Sleeves(IVORY);
            outfit.head = Headwear::HoodDown(GREEN);
            outfit.cloak = Some(Cloak::new(GREEN, IVORY * 0.6));
            outfit.baldric = Some(Baldric {
                color: LEATHER,
                reverse: false,
                buckle: true,
            });
            outfit.skirt = Some(Skirt {
                hem: Hem::Points {
                    count: 9,
                    depth: 0.42,
                },
                layers: [
                    Some(lining(
                        GREEN * 0.7,
                        0.07,
                        Hem::Points {
                            count: 7,
                            depth: 0.3,
                        },
                    )),
                    None,
                ],
                ..skirt(GREEN * 1.4, 0.26, 0.08, Motif::Leaf)
            });
        }
        Wardrobe::Rogue => {
            outfit.tunic = INK;
            outfit.arms = Arms::Sleeves(IVORY);
            outfit.head = Headwear::HoodDown(RED);
            outfit.cloak = Some(Cloak::new(RED, INK));
            outfit.trimmings.garter = Some(LEATHER);
            outfit.baldric = Some(Baldric {
                color: LEATHER,
                reverse: true,
                buckle: true,
            });
            outfit.skirt = Some(Skirt {
                cutaway: true,
                hem: Hem::HighLow(0.35),
                ..skirt(RED, 0.42, 0.16, Motif::Knot)
            });
        }
        Wardrobe::Paladin | Wardrobe::Valkyrie => {
            outfit = Outfit::from_wardrobe(Wardrobe::Knight, wearer);
            outfit.head = Headwear::Bare;
            outfit.tunic = BLUE;
            outfit.trimmings.heraldry = Some(Heraldry {
                field: BLUE,
                edge: GILT,
                emblem: IVORY,
                mark: 3,
            });
            outfit.bodice.cut = if woman {
                Neckline::COURT
            } else {
                Neckline::Closed
            };
            outfit.cloak = Some(Cloak::new(BLUE, IVORY));
            let winged = wardrobe == Wardrobe::Valkyrie;
            outfit.skirt = Some(Skirt {
                hem: if winged {
                    Hem::Points {
                        count: 8,
                        depth: 0.35,
                    }
                } else {
                    Hem::HighLow(0.22)
                },
                panel: Some((IVORY, 0.13)),
                layers: [
                    Some(lining(
                        IVORY,
                        0.055,
                        Hem::Points {
                            count: 8,
                            depth: 0.22,
                        },
                    )),
                    None,
                ],
                ..skirt(BLUE, if winged { 0.48 } else { 0.64 }, 0.12, Motif::Lily)
            });
            if winged {
                outfit.head = Headwear::WingedHelm {
                    metal: STEEL,
                    feathers: BLUE,
                };
                outfit.trimmings.shoulders = Some(Fringe {
                    kind: FringeKind::Feathers,
                    color: IVORY,
                    accent: GILT,
                    length: 0.16,
                });
                outfit.trimmings.peaks = Some(STEEL);
            }
        }
        Wardrobe::Barbarian => {
            let fur = Fringe {
                kind: FringeKind::Fur,
                color: Vec3::new(0.55, 0.41, 0.28),
                accent: IVORY,
                length: 0.13,
            };
            outfit.trimmings.shoulders = Some(fur);
            outfit.trimmings.boot_tops = Some(Fringe {
                length: 0.08,
                ..fur
            });
            outfit.tunic = LEATHER * 0.65;
            outfit.trimmings.armlets = Some(GILT * 0.75);
            outfit.trimmings.waist = Some(Waist::Coins(GILT * 0.8));
            if woman {
                outfit.legs = Legs::Bare(Pigment::Skin(1.0));
            }
            outfit.skirt = Some(Skirt {
                hem: Hem::Points {
                    count: 11,
                    depth: 0.43,
                },
                layers: [
                    Some(lining(
                        LEATHER * 0.6,
                        0.08,
                        Hem::Points {
                            count: 9,
                            depth: 0.38,
                        },
                    )),
                    None,
                ],
                ..skirt(RED * 0.85, 0.40, 0.18, Motif::Knot)
            });
            outfit.head = Headwear::Circlet {
                color: GILT * 0.8,
                gem: None,
                broad: false,
            };
        }
        Wardrobe::Cleric => {
            outfit.tunic = IVORY;
            outfit.arms = sleeve(IVORY, RED, false);
            outfit.bodice.pattern = embroidered(Motif::Sun, GILT);
            outfit.trimmings.neck = Some(Neck::Jewel {
                band: GILT,
                gem: GILT,
            });
            outfit.trimmings.waist = Some(Waist::Crossed(GILT));
            outfit.cloak = Some(Cloak::new(RED, TEAL));
            if woman {
                outfit.legs = Legs::Bare(Pigment::Skin(1.0));
            }
            outfit.skirt = Some(Skirt {
                panel: Some((RED, 0.13)),
                layers: [
                    Some(Layer {
                        split: Some(0.04),
                        ..lining(TEAL, 0.04, Hem::Even)
                    }),
                    None,
                ],
                ..skirt(IVORY, 0.78, if woman { 0.15 } else { 0.09 }, Motif::Sun)
            });
        }
        Wardrobe::Druid => {
            outfit.trimmings.shoulders = Some(Fringe {
                kind: FringeKind::Leaves,
                color: GREEN * 1.6,
                accent: GILT * 0.65,
                length: 0.14,
            });
            outfit.tunic = GREEN * 1.2;
            outfit.torso_ornament = Some(TorsoOrnament::LeafBodice {
                leaves: GREEN * 1.5,
                pin: GILT,
            });
            outfit.head = Headwear::Wreath(GREEN * 1.5);
            outfit.trimmings.armlets = Some(GREEN * 1.4);
            if woman {
                outfit.legs = Legs::Bare(Pigment::Skin(1.0));
            }
            outfit.skirt = Some(Skirt {
                hem: Hem::Points {
                    count: 9,
                    depth: 0.45,
                },
                layers: [
                    Some(lining(
                        Vec3::new(0.47, 0.37, 0.12),
                        0.17,
                        Hem::Points {
                            count: 7,
                            depth: 0.45,
                        },
                    )),
                    Some(lining(IVORY, 0.23, Hem::HighLow(0.42))),
                ],
                ..skirt(GREEN * 1.5, 0.39, 0.14, Motif::Leaf)
            });
        }
        Wardrobe::Bard => {
            outfit.tunic = INK;
            outfit.arms = sleeve(RED, IVORY, true);
            if let Arms::Styled(ref mut sleeve) = outfit.arms {
                sleeve.puff_lining = Some(IVORY);
            }
            outfit.trimmings.waist = Some(Waist::Sash(VIOLET * 1.5));
            outfit.skirt = Some(Skirt {
                cutaway: true,
                hem: Hem::HighLow(0.35),
                layers: [Some(lining(IVORY, 0.04, Hem::HighLow(0.4))), None],
                ..skirt(RED, 0.55, 0.2, Motif::Scroll)
            });
            outfit.bodice.pattern = embroidered(Motif::Scroll, GILT);
        }
        Wardrobe::VampireNoble => {
            outfit.tunic = INK;
            outfit.bodice.pattern = embroidered(Motif::Scroll, IVORY * 0.8);
            outfit.bodice.edge = RED;
            outfit.arms = sleeve(INK, RED, false);
            outfit.trimmings.neck = Some(Neck::Stand(RED));
            outfit.trimmings.peaks = Some(INK);
            outfit.cloak = Some(Cloak::new(INK, RED));
            outfit.skirt = Some(Skirt {
                cutaway: true,
                fullness: 1.3,
                layers: [Some(lining(RED, 0.03, Hem::Even)), None],
                ..skirt(INK, 0.83, 0.16, Motif::Scroll)
            });
        }
        Wardrobe::Necromancer => {
            outfit.tunic = INK;
            outfit.bodice.pattern = embroidered(Motif::Bone, BONE);
            outfit.bodice.edge = BONE;
            outfit.bodice.lacing = Some(BONE);
            outfit.arms = sleeve(INK, VIOLET, false);
            outfit.trimmings.neck = Some(Neck::Stand(INK));
            outfit.trimmings.peaks = Some(INK);
            outfit.skirt = Some(Skirt {
                hem: Hem::Points {
                    count: 13,
                    depth: 0.36,
                },
                layers: [
                    Some(lining(
                        VIOLET,
                        0.08,
                        Hem::Points {
                            count: 17,
                            depth: 0.38,
                        },
                    )),
                    Some(lining(
                        INK,
                        0.12,
                        Hem::Points {
                            count: 11,
                            depth: 0.3,
                        },
                    )),
                ],
                ..skirt(VIOLET * 0.6, 0.62, 0.16, Motif::Bone)
            });
        }
        Wardrobe::Desert => {
            outfit.tunic = TEAL;
            outfit.bodice.pattern = embroidered(Motif::Scroll, GILT);
            outfit.arms = if woman {
                Arms::Bare
            } else {
                Arms::Sleeves(IVORY)
            };
            outfit.legs = Legs::Trousers(IVORY * 0.8);
            outfit.torso_ornament = Some(TorsoOrnament::KnottedSash {
                sash: RED,
                scarf: Some(RED),
            });
            outfit.trimmings.armlets = woman.then_some(GILT);
            outfit.skirt = Some(Skirt {
                hem: Hem::HighLow(0.35),
                panel: Some((TEAL, 0.12)),
                layers: [
                    Some(lining(
                        RED,
                        0.09,
                        Hem::Points {
                            count: 7,
                            depth: 0.25,
                        },
                    )),
                    None,
                ],
                ..skirt(IVORY, 0.46, 0.19, Motif::Scroll)
            });
        }
        _ => unreachable!("study dispatch only accepts study outfits"),
    }
    if !woman {
        tailor_male(&mut outfit, wardrobe, wearer);
    }
    outfit
}

/// Men share palettes and motifs, but wear shirts, coats and trousered panels.
fn tailor_male(outfit: &mut Outfit, wardrobe: Wardrobe, wearer: &Look) {
    outfit.bodice.cut = Neckline::Closed;
    outfit.bodice.male_cut = None;
    outfit.bodice.lacing = None;
    outfit.bodice.support = Support::Loose;
    outfit.trimmings.garter = None;
    match wardrobe {
        Wardrobe::Barbarian => {
            outfit.tunic = wearer.skin;
            outfit.bodice.cover = Cover::Bare;
            outfit.armhole_edge = None;
            outfit.torso_ornament = Some(TorsoOrnament::Harness {
                strap: LEATHER * 0.65,
                ring: GILT * 0.8,
                shoulder_straps: None,
            });
            // The closed baldric crosses the harness's opposite diagonal.
            outfit.baldric = Some(Baldric {
                color: LEATHER * 0.65,
                reverse: false,
                buckle: false,
            });
        }
        Wardrobe::Cleric | Wardrobe::Druid | Wardrobe::Desert => {
            outfit.bodice.cut = Neckline::Plunge {
                depth: 0.45,
                width: 0.085,
                wrap: false,
            };
            outfit.bodice.edge = IVORY;
            if wardrobe == Wardrobe::Cleric {
                outfit.arms = Arms::Sleeves(IVORY);
            } else if wardrobe == Wardrobe::Druid {
                outfit.torso_ornament = None;
                outfit.bodice.pattern = embroidered(Motif::Leaf, GILT * 0.82);
            }
        }
        Wardrobe::Bard => {
            if let Arms::Styled(ref mut sleeve) = outfit.arms {
                sleeve.cut = SleeveCut::Puff;
            }
        }
        Wardrobe::VampireNoble | Wardrobe::Necromancer => {
            outfit.arms = Arms::Sleeves(INK);
        }
        _ => {}
    }
    if let Some(ref mut lower) = outfit.skirt {
        match wardrobe {
            Wardrobe::Paladin | Wardrobe::Valkyrie | Wardrobe::Barbarian => {}
            _ => {
                lower.cutaway = true;
                lower.fullness = 1.0;
                lower.split = 0.20;
                for layer in lower.layers.iter_mut().flatten() {
                    layer.split = Some(lower.split);
                    layer.fullness = Some(lower.fullness);
                }
            }
        }
    }
}
