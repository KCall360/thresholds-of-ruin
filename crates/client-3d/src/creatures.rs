//! Creatures: jointed, hand-carved figures in the props' palette, animated in
//! code. Typed recipes select body plans, proportions, colours and cosmetic
//! details from disclosed appearance data.
//!
//! A rig is a hierarchy of joint entities. Each frame the pose is worked out
//! from the creature's state (idle, walking, acting, hurt, dodging, dying) and
//! written to the joints. Corpses are the same rigs held in their death pose.
use crate::props::{hash, rand, Accent, Builder, Model, Parts, PropLibrary};
use bevy::{math::Affine3A, prelude::*};
use std::f32::consts::{FRAC_PI_2, PI, TAU};
use tor_client_3d_model::CreatureModel;

/// The body plans.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plan {
    Biped,
    Bird,
    Quadruped,
    Centaur,
    Serpent,
    Floater,
    /// Rooted in place: a plant on its stem.
    Rooted,
    Marker,
}

/// What a creature looks like.
#[derive(Clone, Debug)]
pub struct Look {
    pub key: String,
    pub plan: Plan,
    pub scale: f32,
    style: Style,
    skin: Vec3,
    cloth: Vec3,
    trim: Vec3,
    eyes: Option<Accent>,
    /// A glow carried by the creature (staff orb, wisp fire, ice).
    aura: Option<Accent>,
    hunch: f32,
    /// Walk cycles per cell; heavier creatures stride slower.
    stride: f32,
    /// This individual's variation within its kind.
    v: Variant,
    /// The clothes and armour it was dressed in, if not its kind's own.
    outfit: Option<outfit::Outfit>,
}

/// How one individual differs from others of its kind: for people, sex,
/// build, skin, hair and beard; for beasts, coat shade and markings. Rolled
/// from a seed, so the same individual always looks the same.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variant {
    pub woman: bool,
    /// 0 slender, 1 average, 2 sturdy.
    pub build: u8,
    pub skin: u8,
    pub hair: u8,
    pub hair_tone: u8,
    pub beard: bool,
    /// Coat shade and pattern for beasts, 0 to 1 in steps.
    pub shade: u8,
    pub pattern: u8,
}

impl Default for Variant {
    fn default() -> Self {
        Self {
            woman: false,
            build: 1,
            skin: 1,
            hair: 0,
            hair_tone: 1,
            beard: false,
            shade: 2,
            pattern: 0,
        }
    }
}

impl Variant {
    /// Roll an individual from a seed; `people` allows women and men.
    pub fn roll(seed: u64, people: bool) -> Self {
        let pick = |k: u32, n: u8| ((rand(seed, k) * n as f32) as u8).min(n - 1);
        let woman = people && rand(seed, 101) < 0.5;
        Self {
            woman,
            build: pick(102, 3),
            skin: pick(103, 5),
            hair: pick(104, 5),
            hair_tone: pick(105, 6),
            beard: !woman && rand(seed, 106) < 0.4,
            shade: pick(107, 5),
            pattern: pick(108, 4),
        }
    }

    fn code(&self) -> String {
        format!(
            "{}{}{}{}{}{}{}{}",
            u8::from(self.woman),
            self.build,
            self.skin,
            self.hair,
            self.hair_tone,
            u8::from(self.beard),
            self.shade,
            self.pattern
        )
    }

    /// -1 to 1 across the shade steps, for nudging a colour.
    fn shade_f(&self) -> f32 {
        self.shade as f32 / 2.0 - 1.0
    }
}

/// Offline bestiary preview choice; live creatures use their server-owned seed.
pub static PLAYER_LOOK: std::sync::OnceLock<Variant> = std::sync::OnceLock::new();

/// Human skin tones, fair to deep.
const SKINS: [Vec3; 5] = [
    Vec3::new(0.96, 0.78, 0.64),
    Vec3::new(0.88, 0.67, 0.5),
    Vec3::new(0.76, 0.54, 0.37),
    Vec3::new(0.58, 0.39, 0.25),
    Vec3::new(0.42, 0.27, 0.17),
];

/// Hair colours: black, dark brown, chestnut, auburn, blond, grey.
const HAIRS: [Vec3; 6] = [
    Vec3::new(0.07, 0.06, 0.06),
    Vec3::new(0.22, 0.14, 0.09),
    Vec3::new(0.38, 0.22, 0.11),
    Vec3::new(0.52, 0.22, 0.1),
    Vec3::new(0.8, 0.62, 0.34),
    Vec3::new(0.62, 0.62, 0.64),
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    Hero,
    Sentinel,
    Caster,
    Scout,
    Guardian,
    Hobgoblin,
    Zombie,
    Vampire,
    Yeti,
    Emu,
    Quagga,
    Horse,
    Unicorn,
    Centaur,
    Snake,
    Wisp,
    Ice,
    Common,
    Orc,
    Troll,
    Leprechaun,
    Nymph,
    Medusa,
    Phantom,
    Wraith,
    Bat,
    Kestrel,
    Griffin,
    Dragon,
    Jabberwock,
    Aquator,
    Xeroc,
    Flytrap,
    Rattlesnake,
}

impl Style {
    /// Winged creatures always drawn on the wing, never perched.
    fn flies(self) -> bool {
        matches!(self, Style::Bat | Style::Kestrel)
    }

    /// See-through in every light: its normal look, not a hidden state.
    fn ghostly(self) -> bool {
        self == Style::Phantom
    }

    /// The four-legged Rogue beasts built by their own recipes.
    fn rogue_beast(self) -> bool {
        matches!(
            self,
            Style::Griffin | Style::Dragon | Style::Jabberwock | Style::Aquator | Style::Xeroc
        )
    }
}

const LEATHER: Vec3 = Vec3::new(0.34, 0.2, 0.12);
const IRON: Vec3 = Vec3::new(0.42, 0.43, 0.47);
const STEEL: Vec3 = Vec3::new(0.7, 0.72, 0.76);
const GILT: Vec3 = Vec3::new(0.86, 0.66, 0.3);
const CREAM: Vec3 = Vec3::new(0.93, 0.86, 0.68);
const WOOD: Vec3 = Vec3::new(0.45, 0.29, 0.16);
const BONE: Vec3 = Vec3::new(0.86, 0.82, 0.7);
const SKIN: Vec3 = Vec3::new(0.86, 0.66, 0.5);
const DARK: Vec3 = Vec3::new(0.06, 0.05, 0.06);

impl Look {
    /// Resolve the same procedural look for every observer of this individual.
    pub fn of(model: CreatureModel, seed: u64) -> Look {
        Self::with_choice(model, seed, None)
    }

    /// Offline gallery control, never an override for a live server creature.
    pub fn preview(model: CreatureModel, player: bool, seed: u64) -> Look {
        Self::with_choice(
            model,
            seed,
            player.then(|| PLAYER_LOOK.get().copied()).flatten(),
        )
    }

    fn with_choice(model: CreatureModel, seed: u64, choice: Option<Variant>) -> Look {
        let mut look = Self::designed(model);
        let people = matches!(
            look.style,
            Style::Hero
                | Style::Caster
                | Style::Sentinel
                | Style::Scout
                | Style::Vampire
                | Style::Zombie
                | Style::Hobgoblin
                | Style::Centaur
                | Style::Common
                | Style::Orc
                | Style::Troll
                | Style::Leprechaun
                | Style::Nymph
                | Style::Medusa
                | Style::Phantom
                | Style::Wraith
        );
        look.v = if model == CreatureModel::Placeholder {
            Variant::default()
        } else {
            choice.unwrap_or_else(|| Variant::roll(seed, people))
        };
        look.individualise();
        look.key = format!("{}:{}", look.key, look.v.code());
        look.scale *= BODY_SCALE;
        look
    }

    /// The same individual dressed from the wardrobe. Only humanoids that
    /// wear separable clothes can be redressed; the rest are returned as they
    /// are. Each outfit is cut for this wearer and cached apart.
    pub fn dressed(mut self, wardrobe: Wardrobe) -> Look {
        if !outfit::dresses(&self) || wardrobe == Wardrobe::Native {
            return self;
        }
        self.outfit = Some(outfit::Outfit::from_wardrobe(wardrobe, &self));
        self.key = format!("{}:{wardrobe:?}", self.key);
        self
    }

    /// Whether this look can be redressed from the wardrobe.
    pub fn dresses(&self) -> bool {
        outfit::dresses(self)
    }

    /// Apply this individual's variation to the designed look.
    fn individualise(&mut self) {
        let v = self.v;
        let s = v.shade_f();
        // Builds and sexes differ a little in height.
        self.scale *= [0.97, 1.0, 1.03][v.build as usize] * if v.woman { 0.965 } else { 1.0 };
        match self.style {
            Style::Hero | Style::Caster | Style::Sentinel | Style::Scout | Style::Common => {
                self.skin = SKINS[v.skin as usize];
                // Each wears their own colours: scouts pick a tunic from a
                // palette; robes and tabards keep their house hue, lighter or darker.
                match self.style {
                    Style::Scout => {
                        self.cloth = [
                            Vec3::new(0.24, 0.27, 0.18),
                            Vec3::new(0.32, 0.24, 0.15),
                            Vec3::new(0.2, 0.25, 0.28),
                            Vec3::new(0.3, 0.32, 0.2),
                        ][v.pattern as usize];
                    }
                    Style::Caster | Style::Sentinel => self.cloth *= 1.0 + 0.14 * s,
                    _ => {}
                }
            }
            Style::Centaur => {
                // The rider's skin, and the horse's coat from bay to chestnut to dun.
                self.cloth = SKINS[v.skin as usize];
                self.skin = [
                    Vec3::new(0.3, 0.17, 0.09),
                    Vec3::new(0.42, 0.24, 0.12),
                    Vec3::new(0.5, 0.29, 0.15),
                    Vec3::new(0.58, 0.4, 0.22),
                    Vec3::new(0.66, 0.52, 0.32),
                ][v.shade as usize];
            }
            Style::Hobgoblin => {
                self.skin = [
                    Vec3::new(0.82, 0.5, 0.28),
                    Vec3::new(0.76, 0.52, 0.3),
                    Vec3::new(0.68, 0.55, 0.3),
                    Vec3::new(0.84, 0.45, 0.3),
                    Vec3::new(0.62, 0.46, 0.26),
                ][v.skin as usize];
            }
            Style::Zombie => {
                self.skin = [
                    Vec3::new(0.44, 0.6, 0.42),
                    Vec3::new(0.5, 0.58, 0.48),
                    Vec3::new(0.4, 0.52, 0.44),
                    Vec3::new(0.52, 0.6, 0.4),
                    Vec3::new(0.46, 0.54, 0.5),
                ][v.skin as usize];
            }
            Style::Vampire => {
                self.skin = [
                    Vec3::new(0.86, 0.84, 0.9),
                    Vec3::new(0.8, 0.78, 0.86),
                    Vec3::new(0.74, 0.72, 0.8),
                    Vec3::new(0.84, 0.8, 0.82),
                    Vec3::new(0.7, 0.68, 0.76),
                ][v.skin as usize];
            }
            Style::Quagga | Style::Horse => {
                self.skin *= 1.0 + 0.1 * s;
            }
            Style::Unicorn => {
                self.cloth = self.cloth.lerp(Vec3::new(0.2, 0.18, 0.34), 0.3 + 0.3 * s);
            }
            Style::Emu => {
                self.skin *= 1.0 + 0.12 * s;
            }
            Style::Snake => {
                self.skin = [
                    Vec3::new(0.27, 0.46, 0.2),
                    Vec3::new(0.2, 0.42, 0.24),
                    Vec3::new(0.36, 0.44, 0.18),
                    Vec3::new(0.18, 0.4, 0.32),
                    Vec3::new(0.3, 0.5, 0.16),
                ][v.shade as usize];
                self.cloth = [
                    Vec3::new(0.82, 0.74, 0.3),
                    Vec3::new(0.86, 0.8, 0.5),
                    Vec3::new(0.84, 0.56, 0.24),
                    Vec3::new(0.8, 0.76, 0.36),
                ][v.pattern as usize];
            }
            Style::Yeti => {
                self.cloth = [
                    Vec3::new(0.9, 0.91, 0.94),
                    Vec3::new(0.92, 0.89, 0.82),
                    Vec3::new(0.8, 0.83, 0.88),
                    Vec3::new(0.94, 0.93, 0.9),
                    Vec3::new(0.84, 0.82, 0.8),
                ][v.shade as usize];
            }
            Style::Guardian => {
                self.skin = [
                    Vec3::new(0.64, 0.6, 0.55),
                    Vec3::new(0.58, 0.57, 0.58),
                    Vec3::new(0.7, 0.62, 0.5),
                    Vec3::new(0.5, 0.52, 0.56),
                    Vec3::new(0.66, 0.56, 0.5),
                ][v.shade as usize];
            }
            Style::Orc => {
                self.skin = [
                    Vec3::new(0.52, 0.6, 0.26),
                    Vec3::new(0.46, 0.56, 0.24),
                    Vec3::new(0.58, 0.62, 0.3),
                    Vec3::new(0.42, 0.52, 0.28),
                    Vec3::new(0.55, 0.56, 0.22),
                ][v.skin as usize];
            }
            Style::Troll => {
                self.skin = [
                    Vec3::new(0.66, 0.56, 0.24),
                    Vec3::new(0.6, 0.52, 0.22),
                    Vec3::new(0.7, 0.6, 0.3),
                    Vec3::new(0.56, 0.5, 0.26),
                    Vec3::new(0.64, 0.5, 0.2),
                ][v.skin as usize];
            }
            Style::Leprechaun | Style::Nymph => {
                self.skin = SKINS[(v.skin as usize).min(3)];
                if self.style == Style::Leprechaun {
                    // Each wears a coat of their own green.
                    self.cloth = [
                        Vec3::new(0.12, 0.38, 0.22),
                        Vec3::new(0.1, 0.32, 0.18),
                        Vec3::new(0.16, 0.42, 0.2),
                        Vec3::new(0.08, 0.34, 0.26),
                    ][v.pattern as usize];
                } else {
                    self.cloth = [
                        Vec3::new(0.3, 0.5, 0.18),
                        Vec3::new(0.24, 0.44, 0.2),
                        Vec3::new(0.38, 0.52, 0.2),
                        Vec3::new(0.28, 0.48, 0.26),
                    ][v.pattern as usize];
                }
            }
            Style::Medusa => {
                self.skin = [
                    Vec3::new(0.62, 0.66, 0.4),
                    Vec3::new(0.56, 0.62, 0.38),
                    Vec3::new(0.66, 0.64, 0.42),
                    Vec3::new(0.52, 0.58, 0.4),
                    Vec3::new(0.6, 0.6, 0.36),
                ][v.skin as usize];
                self.cloth *= 1.0 + 0.12 * s;
            }
            Style::Phantom | Style::Wraith => {
                self.skin = [
                    Vec3::new(0.66, 0.72, 0.82),
                    Vec3::new(0.6, 0.66, 0.78),
                    Vec3::new(0.7, 0.74, 0.8),
                    Vec3::new(0.62, 0.7, 0.76),
                    Vec3::new(0.56, 0.62, 0.74),
                ][v.skin as usize];
                self.cloth *= 1.0 + 0.1 * s;
            }
            Style::Bat
            | Style::Kestrel
            | Style::Griffin
            | Style::Dragon
            | Style::Jabberwock
            | Style::Aquator
            | Style::Xeroc
            | Style::Flytrap => {
                self.skin *= 1.0 + 0.08 * s;
            }
            Style::Rattlesnake => {
                self.skin *= 1.0 + 0.1 * s;
                self.cloth *= 1.0 - 0.06 * s;
            }
            _ => {}
        }
    }

    /// The look as designed, a person about 1.85 units tall.
    fn designed(model: CreatureModel) -> Look {
        let base = |plan, style, skin, cloth, trim| Look {
            key: format!("{style:?}:{model:?}"),
            plan,
            scale: 1.0,
            style,
            skin,
            cloth,
            trim,
            eyes: None,
            aura: None,
            hunch: 0.0,
            stride: 1.0,
            v: Variant::default(),
            outfit: None,
        };
        let teal = Vec3::new(0.045, 0.6, 0.55);
        if model == CreatureModel::Placeholder {
            return base(Plan::Marker, Style::Common, SKIN, teal, LEATHER);
        }
        if model == CreatureModel::Human {
            return base(
                Plan::Biped,
                Style::Hero,
                SKIN,
                teal,
                Vec3::new(0.1, 0.3, 0.3),
            );
        }
        if matches!(model, CreatureModel::Bird | CreatureModel::Emu) {
            return Look {
                scale: 0.95,
                ..base(
                    Plan::Bird,
                    Style::Emu,
                    Vec3::new(0.62, 0.38, 0.17),
                    Vec3::new(0.72, 0.75, 0.8),
                    Vec3::new(0.62, 0.62, 0.66),
                )
            };
        }
        if matches!(model, CreatureModel::Serpent | CreatureModel::Snake) {
            return Look {
                scale: 1.45,
                ..base(
                    Plan::Serpent,
                    Style::Snake,
                    Vec3::new(0.27, 0.46, 0.2),
                    Vec3::new(0.82, 0.74, 0.3),
                    Vec3::new(0.85, 0.8, 0.62),
                )
            };
        }
        if matches!(model, CreatureModel::Unicorn | CreatureModel::BlackUnicorn) {
            return Look {
                scale: 1.1,
                eyes: Some(Accent::Violet),
                aura: Some(Accent::Violet),
                stride: 0.8,
                ..base(
                    Plan::Quadruped,
                    Style::Unicorn,
                    // Display-space charcoal remains black after the shared
                    // sRGB decode, with enough blue-violet value to show form.
                    Vec3::new(0.16, 0.17, 0.235),
                    Vec3::new(0.3, 0.24, 0.42),
                    Vec3::new(0.24, 0.25, 0.31),
                )
            };
        }
        if matches!(model, CreatureModel::Quagga | CreatureModel::Equine) {
            return Look {
                stride: 0.85,
                ..base(
                    Plan::Quadruped,
                    if model == CreatureModel::Quagga {
                        Style::Quagga
                    } else {
                        Style::Horse
                    },
                    Vec3::new(0.55, 0.38, 0.22),
                    Vec3::new(0.88, 0.84, 0.74),
                    Vec3::new(0.2, 0.14, 0.1),
                )
            };
        }
        if model == CreatureModel::Centaur {
            return Look {
                scale: 1.05,
                stride: 0.8,
                ..base(
                    Plan::Centaur,
                    Style::Centaur,
                    Vec3::new(0.5, 0.29, 0.15),
                    SKIN * 0.92,
                    LEATHER,
                )
            };
        }
        if model == CreatureModel::Wisp {
            return Look {
                aura: Some(Accent::Ember),
                ..base(
                    Plan::Floater,
                    Style::Wisp,
                    Vec3::new(1.0, 0.5, 0.2),
                    Vec3::ONE,
                    Vec3::ONE,
                )
            };
        }
        if model == CreatureModel::Ice {
            return Look {
                scale: 1.15,
                aura: Some(Accent::Travel),
                ..base(
                    Plan::Floater,
                    Style::Ice,
                    Vec3::new(0.7, 0.85, 0.95),
                    Vec3::ONE,
                    Vec3::ONE,
                )
            };
        }
        if model == CreatureModel::Yeti {
            return Look {
                scale: 1.22,
                hunch: 0.3,
                stride: 0.8,
                eyes: Some(Accent::Travel),
                ..base(
                    Plan::Biped,
                    Style::Yeti,
                    Vec3::new(0.26, 0.29, 0.38),
                    Vec3::new(0.9, 0.91, 0.94),
                    Vec3::new(0.75, 0.8, 0.86),
                )
            };
        }
        if model == CreatureModel::Guardian {
            return Look {
                scale: 1.18,
                eyes: Some(Accent::Teal),
                stride: 0.75,
                ..base(
                    Plan::Biped,
                    Style::Guardian,
                    Vec3::new(0.64, 0.6, 0.55),
                    Vec3::new(0.55, 0.52, 0.48),
                    Vec3::new(0.3, 0.42, 0.28),
                )
            };
        }
        if let Some(look) = Self::rogue(model, &base) {
            return look;
        }
        if model == CreatureModel::Hobgoblin {
            return Look {
                scale: 0.95,
                hunch: 0.15,
                eyes: None,
                ..base(
                    Plan::Biped,
                    Style::Hobgoblin,
                    Vec3::new(0.8, 0.52, 0.3),
                    Vec3::new(0.3, 0.27, 0.22),
                    IRON,
                )
            };
        }
        if model == CreatureModel::Zombie {
            return Look {
                hunch: 0.25,
                stride: 0.7,
                eyes: Some(Accent::White),
                ..base(
                    Plan::Biped,
                    Style::Zombie,
                    Vec3::new(0.44, 0.6, 0.42),
                    Vec3::new(0.35, 0.3, 0.24),
                    Vec3::new(0.25, 0.22, 0.2),
                )
            };
        }
        if model == CreatureModel::Vampire {
            return Look {
                eyes: Some(Accent::Danger),
                ..base(
                    Plan::Biped,
                    Style::Vampire,
                    Vec3::new(0.86, 0.84, 0.9),
                    DARK,
                    Vec3::new(0.5, 0.06, 0.08),
                )
            };
        }
        if matches!(
            model,
            CreatureModel::Sentinel | CreatureModel::RedSentinel | CreatureModel::BlueSentinel
        ) {
            let tabard = match model {
                CreatureModel::RedSentinel => Vec3::new(0.66, 0.16, 0.13),
                CreatureModel::BlueSentinel => Vec3::new(0.18, 0.3, 0.68),
                _ => Vec3::new(0.4, 0.4, 0.42),
            };
            return base(Plan::Biped, Style::Sentinel, SKIN, tabard, STEEL);
        }
        if matches!(model, CreatureModel::Mage | CreatureModel::RedMage) {
            let robe = if model == CreatureModel::RedMage {
                Vec3::new(0.66, 0.16, 0.13)
            } else {
                Vec3::new(0.22, 0.42, 0.44)
            };
            return Look {
                aura: None,
                ..base(
                    Plan::Biped,
                    Style::Caster,
                    SKIN,
                    robe,
                    Vec3::new(0.9, 0.75, 0.4),
                )
            };
        }
        if model == CreatureModel::Scout {
            return base(
                Plan::Biped,
                Style::Scout,
                SKIN * 0.9,
                Vec3::new(0.24, 0.27, 0.18),
                LEATHER,
            );
        }
        let tint = Vec3::new(0.4, 0.48, 0.5);
        base(Plan::Biped, Style::Common, SKIN, tint * 0.6, LEATHER)
    }

    /// The Rogue scenario's own monsters, each from its concept sheet.
    fn rogue(
        model: CreatureModel,
        base: &dyn Fn(Plan, Style, Vec3, Vec3, Vec3) -> Look,
    ) -> Option<Look> {
        let rgb = Vec3::new;
        let look = match model {
            CreatureModel::Orc => Look {
                scale: 1.04,
                hunch: 0.08,
                stride: 0.9,
                ..base(
                    Plan::Biped,
                    Style::Orc,
                    rgb(0.52, 0.6, 0.26),
                    rgb(0.36, 0.22, 0.13),
                    rgb(0.62, 0.12, 0.12),
                )
            },
            CreatureModel::Troll => Look {
                scale: 1.14,
                hunch: 0.22,
                stride: 0.8,
                ..base(
                    Plan::Biped,
                    Style::Troll,
                    rgb(0.66, 0.56, 0.24),
                    rgb(0.36, 0.38, 0.22),
                    rgb(0.32, 0.18, 0.12),
                )
            },
            CreatureModel::Leprechaun => Look {
                scale: 0.76,
                stride: 1.25,
                ..base(
                    Plan::Biped,
                    Style::Leprechaun,
                    SKIN,
                    rgb(0.12, 0.38, 0.22),
                    GILT,
                )
            },
            CreatureModel::Nymph => Look {
                scale: 0.96,
                stride: 1.05,
                ..base(
                    Plan::Biped,
                    Style::Nymph,
                    SKIN,
                    rgb(0.3, 0.5, 0.18),
                    rgb(0.8, 0.66, 0.25),
                )
            },
            CreatureModel::Medusa => Look {
                eyes: Some(Accent::Amber),
                ..base(
                    Plan::Biped,
                    Style::Medusa,
                    rgb(0.62, 0.66, 0.4),
                    rgb(0.42, 0.12, 0.24),
                    rgb(0.78, 0.52, 0.24),
                )
            },
            CreatureModel::Phantom => Look {
                eyes: Some(Accent::Travel),
                stride: 0.9,
                ..base(
                    Plan::Biped,
                    Style::Phantom,
                    rgb(0.66, 0.72, 0.82),
                    rgb(0.36, 0.3, 0.55),
                    rgb(0.24, 0.2, 0.36),
                )
            },
            CreatureModel::Wraith => Look {
                scale: 1.04,
                hunch: 0.1,
                eyes: Some(Accent::Teal),
                stride: 0.85,
                ..base(
                    Plan::Biped,
                    Style::Wraith,
                    rgb(0.62, 0.68, 0.76),
                    rgb(0.18, 0.2, 0.26),
                    rgb(0.18, 0.42, 0.42),
                )
            },
            CreatureModel::Bat => Look {
                scale: 0.82,
                stride: 1.4,
                ..base(
                    Plan::Bird,
                    Style::Bat,
                    rgb(0.56, 0.32, 0.18),
                    rgb(0.42, 0.2, 0.26),
                    rgb(0.86, 0.74, 0.56),
                )
            },
            CreatureModel::Kestrel => Look {
                scale: 0.8,
                stride: 1.4,
                ..base(
                    Plan::Bird,
                    Style::Kestrel,
                    rgb(0.72, 0.38, 0.18),
                    rgb(0.92, 0.8, 0.62),
                    rgb(0.52, 0.56, 0.62),
                )
            },
            CreatureModel::Griffin => Look {
                scale: 1.06,
                stride: 0.8,
                ..base(
                    Plan::Quadruped,
                    Style::Griffin,
                    rgb(0.74, 0.5, 0.24),
                    rgb(0.94, 0.88, 0.72),
                    rgb(0.12, 0.42, 0.42),
                )
            },
            CreatureModel::Dragon => Look {
                scale: 1.22,
                stride: 0.65,
                ..base(
                    Plan::Quadruped,
                    Style::Dragon,
                    rgb(0.72, 0.2, 0.12),
                    rgb(0.9, 0.74, 0.52),
                    rgb(0.5, 0.12, 0.14),
                )
            },
            CreatureModel::Jabberwock => Look {
                scale: 1.12,
                stride: 0.75,
                ..base(
                    Plan::Quadruped,
                    Style::Jabberwock,
                    rgb(0.32, 0.46, 0.26),
                    rgb(0.88, 0.78, 0.6),
                    rgb(0.9, 0.48, 0.16),
                )
            },
            CreatureModel::Aquator => Look {
                scale: 0.95,
                stride: 1.6,
                ..base(
                    Plan::Quadruped,
                    Style::Aquator,
                    rgb(0.16, 0.5, 0.52),
                    rgb(0.9, 0.84, 0.66),
                    rgb(0.9, 0.42, 0.12),
                )
            },
            CreatureModel::Xeroc => Look {
                scale: 0.92,
                stride: 1.7,
                ..base(
                    Plan::Quadruped,
                    Style::Xeroc,
                    rgb(0.46, 0.28, 0.34),
                    rgb(0.78, 0.66, 0.48),
                    rgb(0.62, 0.16, 0.2),
                )
            },
            CreatureModel::Flytrap => base(
                Plan::Rooted,
                Style::Flytrap,
                rgb(0.36, 0.58, 0.22),
                rgb(0.72, 0.18, 0.16),
                rgb(0.94, 0.88, 0.66),
            ),
            CreatureModel::Rattlesnake => Look {
                scale: 1.45,
                ..base(
                    Plan::Serpent,
                    Style::Rattlesnake,
                    rgb(0.5, 0.48, 0.24),
                    rgb(0.78, 0.64, 0.36),
                    rgb(0.88, 0.82, 0.62),
                )
            },
            _ => return None,
        };
        Some(look)
    }

    /// Whether this individual is a woman.
    pub fn woman(&self) -> bool {
        self.v.woman
    }

    /// Height of the head's top, for labels.
    pub fn height(&self) -> f32 {
        let h = match (self.plan, self.style) {
            (_, Style::Bat | Style::Kestrel) => 1.9,
            (_, Style::Dragon) => 1.55,
            (_, Style::Griffin) => 1.45,
            (_, Style::Jabberwock) => 1.8,
            (_, Style::Aquator) => 0.85,
            (_, Style::Xeroc) => 0.72,
            (Plan::Biped, _) => 1.85,
            (Plan::Bird, _) => 1.75,
            (Plan::Quadruped, _) => 1.7,
            (Plan::Centaur, _) => 2.2,
            (Plan::Serpent, _) => 0.6,
            (Plan::Floater, _) => 1.6,
            (Plan::Rooted, _) => 1.3,
            (Plan::Marker, _) => 1.0,
        };
        h * self.scale
    }

    pub fn light(&self) -> Option<(Color, f32)> {
        match self.style {
            Style::Wisp => Some((Accent::Ember.light(), 30_000.0)),
            Style::Ice => Some((Accent::Travel.light(), 18_000.0)),
            Style::Caster => self.aura.map(|a| (a.light(), 6_000.0)),
            Style::Unicorn => Some((Accent::Violet.light(), 5_000.0)),
            _ => None,
        }
    }
}

/// Bold Storybook visual presence relative to dungeon cells. This is an art
/// scale, not a conversion to real-world feet or authoritative occupied cells.
pub const BODY_SCALE: f32 = 0.74;

/// The joints a rig may have.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub enum J {
    Lift,
    Hips,
    Torso,
    Head,
    ArmL,
    ForeL,
    HandL,
    ArmR,
    ForeR,
    HandR,
    LegL,
    ShinL,
    LegR,
    ShinR,
    Body,
    Neck,
    Snout,
    Tail,
    FL,
    FLs,
    FR,
    FRs,
    BL,
    BLs,
    BR,
    BRs,
    Core,
    FootL,
    FootR,
    HoofFL,
    HoofFR,
    HoofBL,
    HoofBR,
    /// Wings: the arm from the shoulder, and the hand that carries the tip.
    WingL,
    WingR,
    WingTipL,
    WingTipR,
    /// A lower jaw, or a flytrap's lower lobe.
    Jaw,
}
const JOINTS: usize = 38;

#[derive(Component)]
pub struct Rig {
    pub plan: Plan,
    /// Resolved anatomy; shared by mesh generation, fitting and support.
    humanoid: Option<anatomy::HumanoidBody>,
    joints: [Option<Entity>; JOINTS],
    rest: [Vec3; JOINTS],
    /// Serpent segments or floater motes, posed directly.
    segments: Vec<Entity>,
    /// Continuous caster garment with an owned mesh and shared material.
    robe: Option<robe::RobeRig>,
    stride: f32,
    hunch: f32,
    style: Style,
    /// Overall size, applied to whole-body offsets (lunges, falls).
    scale: f32,
    /// A four-legged beast's ankle height with its soles on the floor.
    ankle_floor: f32,
    /// Leg length relative to a horse's, scaling crouch and step height.
    legs: f32,
    /// Half the body's width, the height it lies at when fallen on its side.
    girth: f32,
    /// Hanging cloth that swings on its own as the wearer moves.
    drapes: Vec<(Entity, Drape)>,
}

/// How a piece of hanging cloth swings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Drape {
    /// A cloak or cape from the shoulders: it flares back as its wearer
    /// travels and trails through turns.
    Cloak,
    /// A skirt, kilt, coat tails or rags from the hips, of this length: it
    /// sways with the stride, longer cloth more slowly and less far.
    Skirt(f32),
}

impl Rig {
    pub fn joint(&self, j: J) -> Option<Entity> {
        self.joints[j as usize]
    }
}

/// What a creature is doing: one motion for each kind of ability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
    /// Neutral action for an unfamiliar authored technique.
    Gesture,
    /// A plain swing, bite, peck or kick.
    Strike,
    /// A straight lunge with the point (Measured Cut).
    Thrust,
    /// A follow-up blow from the other side (Second and Third Strike).
    Combo(bool),
    /// A low twisting hook at a joint (Dislocating Strike).
    Hook,
    /// A shield punch that knocks the guard aside (Open the Guard).
    Bash,
    /// Driving forward with the whole body (Pressure).
    Shove,
    /// An overhead smash (Rattling Blow).
    Heavy,
    /// Studying the foe, weapon raised (Read the Rhythm).
    Focus,
    /// A spell flung at the target (Spark Lance).
    Cast(Accent),
    /// A curse woven at the target (Falter).
    Hex,
    /// A terrible cry (Fear).
    Fear,
    /// Life pulled out of the target (Drain Vitality).
    Drain,
    /// A grasping touch that burns or freezes (Freezing Touch).
    Touch(Accent),
    /// Bracing or warding oneself (Brace for Impact, Shell).
    Guard(Accent),
    /// A careful step back behind the guard (Guarded Step).
    Step,
    /// Kneeling to bind one's wounds (Bind Wounds).
    Mend,
    /// Fire or frost breathed from an open jaw (Breath Weapon).
    Breath(Accent),
    /// A confusing stare (Confusing Gaze).
    Gaze,
    /// A venomous or corroding bite.
    Bite(Accent),
}

impl Move {
    /// Recipes use disclosed content identity; captions never choose mechanics.
    pub fn for_ability(id: &str, strike: Option<u16>) -> Move {
        match id {
            "basic_melee" if strike.is_some_and(|s| s > 0) => Move::Combo(strike.unwrap() % 2 == 1),
            "basic_melee" => Move::Strike,
            "measured_cut" => Move::Thrust,
            "dislocating_strike" => Move::Hook,
            "open_the_guard" => Move::Bash,
            "rattling_blow" => Move::Heavy,
            "read_the_rhythm" => Move::Focus,
            "spark_lance_fire" => Move::Cast(Accent::Ember),
            "spark_lance_cold" => Move::Cast(Accent::Travel),
            "falter" => Move::Hex,
            "rogue.fear" => Move::Fear,
            "rogue.drain_vitality" => Move::Drain,
            "rogue.freezing_touch" => Move::Touch(Accent::Travel),
            "rogue.breath_weapon" => Move::Breath(Accent::Ember),
            "rogue.confusing_gaze" => Move::Gaze,
            "rogue.corrode" => Move::Touch(Accent::Amber),
            "rogue.energy_drain" => Move::Drain,
            "rogue.ambush_from_guise" => Move::Shove,
            "rogue.venomous_bite" => Move::Bite(Accent::Teal),
            "brace_for_impact" => Move::Guard(Accent::Gold),
            "shell_impact" | "shell_keen" | "shell_energy" => Move::Guard(Accent::Teal),
            "guarded_step" => Move::Step,
            "bind_wounds" => Move::Mend,
            _ => Move::Gesture,
        }
    }

    pub fn duration(self) -> f32 {
        match self {
            Move::Gesture => 0.8,
            Move::Strike => 0.62,
            Move::Thrust => 0.5,
            Move::Combo(_) => 0.55,
            Move::Hook => 0.65,
            Move::Bash => 0.6,
            Move::Shove => 0.65,
            Move::Heavy => 0.9,
            Move::Focus => 1.1,
            Move::Cast(_) => 0.85,
            Move::Hex => 0.9,
            Move::Fear => 0.95,
            Move::Drain => 1.15,
            Move::Touch(_) => 0.7,
            Move::Guard(_) => 1.1,
            Move::Step => 0.8,
            Move::Mend => 1.5,
            Move::Breath(_) => 1.2,
            Move::Gaze => 1.0,
            Move::Bite(_) => 0.6,
        }
    }

    /// When in the move the blow lands or the spell leaves the hand.
    pub fn impact(self) -> f32 {
        match self {
            Move::Gesture => 0.4,
            Move::Strike => 0.42,
            Move::Thrust => 0.38,
            Move::Combo(_) => 0.38,
            Move::Hook => 0.45,
            Move::Bash => 0.4,
            Move::Shove => 0.42,
            Move::Heavy => 0.55,
            Move::Focus => 0.5,
            Move::Cast(_) => 0.45,
            Move::Hex => 0.5,
            Move::Fear => 0.4,
            Move::Drain => 0.35,
            Move::Touch(_) => 0.45,
            Move::Guard(_) => 0.25,
            Move::Step => 0.3,
            Move::Mend => 0.6,
            Move::Breath(_) => 0.5,
            Move::Gaze => 0.5,
            Move::Bite(_) => 0.42,
        }
    }

    /// Whether the move is aimed at someone else.
    pub fn offensive(self) -> bool {
        !matches!(
            self,
            Move::Gesture | Move::Guard(_) | Move::Step | Move::Focus | Move::Mend
        )
    }
}

/// Renderer-only support memory. Each leg retains one ankle point and its
/// landing heading; swing and settling derive their targets from that point.
#[derive(Clone, Copy)]
struct WalkFoot {
    world: Vec3,
    yaw: f32,
    phase: f32,
    lifted: f32,
    settle_order: u8,
    settled: bool,
}

#[derive(Default)]
struct WalkContacts {
    rig: Option<Entity>,
    feet: [Option<WalkFoot>; 4],
    blend: f32,
    settle: Option<f32>,
}

#[derive(Component)]
pub struct Anim {
    pub yaw: f32,
    pub target_yaw: Option<f32>,
    walk: f32,
    speed: f32,
    contacts: WalkContacts,
    last: Vec3,
    time: f32,
    seed: f32,
    pub action: Option<(Move, f32)>,
    /// Seconds until a hit lands (negative while waiting), then its age.
    hurt: Option<f32>,
    dodge: Option<f32>,
    /// Death progress, 0 to 1. A corpse holds 1.
    pub dead: Option<f32>,
    /// How far down it still is after being revived: it rises out of its
    /// death pose instead of snapping upright.
    fallen: f32,
    /// How fully the idle fidgets are in play, easing between actions.
    rest: f32,
    /// Hold the rest pose without fidgets, for portraits.
    pub quiet: bool,
}

impl Anim {
    pub fn new(seed: u64, at: Vec3) -> Self {
        Self {
            yaw: rand(seed, 1) * TAU,
            target_yaw: None,
            walk: 0.0,
            speed: 0.0,
            contacts: WalkContacts::default(),
            last: at,
            time: 0.0,
            seed: rand(seed, 2) * 10.0,
            action: None,
            hurt: None,
            dodge: None,
            dead: None,
            fallen: 0.0,
            rest: 1.0,
            quiet: false,
        }
    }

    pub fn corpse(seed: u64) -> Self {
        Self {
            dead: Some(1.0),
            ..Self::new(seed, Vec3::ZERO)
        }
    }

    pub fn act(&mut self, action: Move) {
        self.contacts = WalkContacts::default();
        self.action = Some((action, 0.0));
    }

    pub fn hit(&mut self, delay: f32) {
        self.hurt = Some(-delay);
    }

    pub fn miss(&mut self, delay: f32) {
        self.dodge = Some(-delay);
    }
}

mod anatomy;
mod animation;
#[cfg(test)]
mod art_contract_tests;
mod corpse;
mod effects;
mod geometry;
mod outfit;
#[cfg(test)]
mod reaction_tests;
mod robe;
#[cfg(test)]
mod rogue_tests;

pub use animation::{animate, Boredom, BORED_AFTER};
pub use corpse::{snuff, spawn_corpse, Snuffed};
pub use effects::{effects, stage, Bolt, Fade};
pub use geometry::spawn;
pub use outfit::Wardrobe;

/// Convert only the shared cosmetic seed to this renderer's variation stream.
/// Unknown appearance uses one neutral recipe; interaction targets never seed it.
pub fn individual(seed: Option<tor_protocol::AppearanceSeed>) -> u64 {
    seed.map_or(0, |seed| hash(&seed.to_string()))
}

/// A seed that stays the same for a creature of this name.
pub fn seed(name: &str) -> u64 {
    hash(name)
}
