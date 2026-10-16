//! Creatures: jointed, hand-carved figures in the props' palette, animated in
//! code. Five body plans cover every creature in the scenarios; each creature
//! is a set of choices on one of them (proportions, colours, gear, glow).
//!
//! A rig is a hierarchy of joint entities. Each frame the pose is worked out
//! from the creature's state (idle, walking, acting, hurt, dodging, dying) and
//! written to the joints. Corpses are the same rigs held in their death pose.
use crate::props::{hash, rand, Accent, Builder, Model, Parts, PropLibrary};
use bevy::{math::Affine3A, prelude::*};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// The body plans.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plan {
    Biped,
    Bird,
    Quadruped,
    Centaur,
    Serpent,
    Floater,
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
        Self { woman: false, build: 1, skin: 1, hair: 0, hair_tone: 1, beard: false, shade: 2, pattern: 0 }
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

/// The player's chosen appearance, set once at start-up.
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
    Unicorn,
    Centaur,
    Snake,
    Wisp,
    Ice,
    Common,
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

fn faction(name: &str, otherwise: Vec3) -> Vec3 {
    if name.contains("red") {
        Vec3::new(0.66, 0.16, 0.13)
    } else if name.contains("blue") {
        Vec3::new(0.18, 0.3, 0.68)
    } else {
        otherwise
    }
}

fn faction_glow(name: &str, otherwise: Accent) -> Accent {
    if name.contains("red") {
        Accent::Ember
    } else if name.contains("blue") {
        Accent::Travel
    } else {
        otherwise
    }
}

impl Look {
    /// Choose a look from a creature's name; `player` marks the observer,
    /// whose appearance is their own choice. `seed` picks this individual.
    pub fn of(name: &str, player: bool, seed: u64) -> Look {
        let mut look = Self::designed(name, player);
        let people = matches!(
            look.style,
            Style::Hero | Style::Caster | Style::Sentinel | Style::Scout | Style::Vampire | Style::Zombie | Style::Hobgoblin | Style::Centaur | Style::Common
        );
        look.v = if player { PLAYER_LOOK.get().copied().unwrap_or_default() } else { Variant::roll(seed, people) };
        look.individualise();
        look.key = format!("{}:{}", look.key, look.v.code());
        look.scale *= BODY_SCALE;
        look
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
            Style::Quagga => {
                self.skin *= 1.0 + 0.1 * s;
            }
            Style::Unicorn => {
                self.cloth = self.cloth.lerp(Vec3::new(0.1, 0.12, 0.26), 0.3 + 0.3 * s);
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
            _ => {}
        }
    }

    /// The look as designed, a person about 1.85 units tall.
    fn designed(name: &str, player: bool) -> Look {
        let n = name.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
        let base = |plan, style, skin, cloth, trim| Look {
            key: format!("{style:?}:{}", if player { "player".to_string() } else { n.clone() }),
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
        };
        let teal = Vec3::new(0.16, 0.5, 0.46);
        if player || has(&["warrior", "delver", "hero"]) && !has(&["mage"]) {
            return base(Plan::Biped, Style::Hero, SKIN, teal, Vec3::new(0.1, 0.3, 0.3));
        }
        if has(&["emu", "ostrich", "bird"]) {
            return Look {
                scale: 0.95,
                ..base(Plan::Bird, Style::Emu, Vec3::new(0.62, 0.38, 0.17), Vec3::new(0.72, 0.75, 0.8), Vec3::new(0.62, 0.62, 0.66))
            };
        }
        if has(&["snake", "serpent", "viper", "adder"]) {
            return Look {
                scale: 1.45,
                ..base(Plan::Serpent, Style::Snake, Vec3::new(0.27, 0.46, 0.2), Vec3::new(0.82, 0.74, 0.3), Vec3::new(0.85, 0.8, 0.62))
            };
        }
        if has(&["unicorn"]) {
            return Look {
                scale: 1.1,
                eyes: Some(Accent::Violet),
                aura: Some(Accent::Violet),
                stride: 0.8,
                ..base(Plan::Quadruped, Style::Unicorn, Vec3::new(0.05, 0.05, 0.08), Vec3::new(0.16, 0.12, 0.22), Vec3::new(0.2, 0.2, 0.24))
            };
        }
        if has(&["quagga", "zebra", "horse", "pony"]) {
            return Look {
                stride: 0.85,
                ..base(Plan::Quadruped, Style::Quagga, Vec3::new(0.55, 0.38, 0.22), Vec3::new(0.88, 0.84, 0.74), Vec3::new(0.2, 0.14, 0.1))
            };
        }
        if has(&["centaur"]) {
            return Look {
                scale: 1.05,
                stride: 0.8,
                ..base(Plan::Centaur, Style::Centaur, Vec3::new(0.5, 0.29, 0.15), SKIN * 0.92, LEATHER)
            };
        }
        if has(&["wisp", "ember", "flame", "fire"]) {
            return Look {
                aura: Some(Accent::Ember),
                ..base(Plan::Floater, Style::Wisp, Vec3::new(1.0, 0.5, 0.2), Vec3::ONE, Vec3::ONE)
            };
        }
        if has(&["ice", "frost"]) {
            return Look {
                scale: 1.15,
                aura: Some(Accent::Travel),
                ..base(Plan::Floater, Style::Ice, Vec3::new(0.7, 0.85, 0.95), Vec3::ONE, Vec3::ONE)
            };
        }
        if has(&["yeti", "sasquatch"]) {
            return Look {
                scale: 1.22,
                hunch: 0.3,
                stride: 0.8,
                eyes: Some(Accent::Travel),
                ..base(Plan::Biped, Style::Yeti, Vec3::new(0.26, 0.29, 0.38), Vec3::new(0.9, 0.91, 0.94), Vec3::new(0.75, 0.8, 0.86))
            };
        }
        if has(&["guardian", "construct", "golem", "statue"]) {
            return Look {
                scale: 1.18,
                eyes: Some(Accent::Teal),
                stride: 0.75,
                ..base(Plan::Biped, Style::Guardian, Vec3::new(0.64, 0.6, 0.55), Vec3::new(0.55, 0.52, 0.48), Vec3::new(0.3, 0.42, 0.28))
            };
        }
        if has(&["hobgoblin", "goblin", "orc", "kobold"]) {
            return Look {
                scale: 0.95,
                hunch: 0.15,
                eyes: None,
                ..base(Plan::Biped, Style::Hobgoblin, Vec3::new(0.8, 0.52, 0.3), Vec3::new(0.3, 0.27, 0.22), IRON)
            };
        }
        if has(&["zombie", "ghoul", "corpse"]) {
            return Look {
                hunch: 0.25,
                stride: 0.7,
                eyes: Some(Accent::White),
                ..base(Plan::Biped, Style::Zombie, Vec3::new(0.44, 0.6, 0.42), Vec3::new(0.35, 0.3, 0.24), Vec3::new(0.25, 0.22, 0.2))
            };
        }
        if has(&["vampire", "lich", "wraith"]) {
            return Look {
                eyes: Some(Accent::Danger),
                ..base(Plan::Biped, Style::Vampire, Vec3::new(0.86, 0.84, 0.9), DARK, Vec3::new(0.5, 0.06, 0.08))
            };
        }
        if has(&["sentinel", "knight", "guard", "soldier"]) {
            let tabard = faction(&n, Vec3::new(0.4, 0.4, 0.42));
            return base(Plan::Biped, Style::Sentinel, SKIN, tabard, STEEL);
        }
        if has(&["arcanist", "mage", "adept", "wizard", "witch", "priest"]) {
            let robe = faction(&n, Vec3::new(0.22, 0.42, 0.44));
            return Look {
                aura: Some(faction_glow(&n, Accent::Teal)),
                ..base(Plan::Biped, Style::Caster, SKIN, robe, Vec3::new(0.9, 0.75, 0.4))
            };
        }
        if has(&["scout", "thief", "rogue", "bandit", "scavenger"]) {
            return base(Plan::Biped, Style::Scout, SKIN * 0.9, Vec3::new(0.24, 0.27, 0.18), LEATHER);
        }
        let tint = crate::creature_tint(name);
        base(Plan::Biped, Style::Common, SKIN, tint * 0.6, LEATHER)
    }

    /// Whether this individual is a woman.
    pub fn woman(&self) -> bool {
        self.v.woman
    }

    /// Height of the head's top, for labels.
    pub fn height(&self) -> f32 {
        let h = match self.plan {
            Plan::Biped => 1.85,
            Plan::Bird => 1.75,
            Plan::Quadruped => 1.7,
            Plan::Centaur => 2.2,
            Plan::Serpent => 0.6,
            Plan::Floater => 1.6,
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

/// Cells are five-foot cubes; a figure designed 1.85 units tall stands
/// about five foot nine. A body two cells high only reaches into the second.
pub const BODY_SCALE: f32 = 0.62;

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
}
const JOINTS: usize = 27;

#[derive(Component)]
pub struct Rig {
    pub plan: Plan,
    joints: [Option<Entity>; JOINTS],
    rest: [Vec3; JOINTS],
    /// Serpent segments or floater motes, posed directly.
    segments: Vec<Entity>,
    stride: f32,
    hunch: f32,
    style: Style,
    /// Overall size, applied to whole-body offsets (lunges, falls).
    scale: f32,
}

impl Rig {
    pub fn joint(&self, j: J) -> Option<Entity> {
        self.joints[j as usize]
    }
}

/// What a creature is doing: one motion for each kind of ability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Move {
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
}

impl Move {
    /// Read an ability's name for the motion that fits it. `strike` is the
    /// server's ordinal of the blow within a sequence.
    pub fn of(name: &str, self_targeted: bool, strike: Option<u16>) -> Move {
        let n = name.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
        let element = if has(&["fire", "spark", "ember", "flame", "burn"]) && !has(&["cold"]) {
            Accent::Ember
        } else if has(&["frost", "ice", "freez", "cold", "chill"]) {
            Accent::Travel
        } else {
            Accent::Violet
        };
        if has(&["bind wound", "bandage", "heal", "mend", "regenerat", "recover"]) {
            Move::Mend
        } else if has(&["guarded step", "retreat", "withdraw"]) {
            Move::Step
        } else if has(&["rhythm", "study", "focus", "aim", "watch"]) {
            Move::Focus
        } else if has(&["ward", "shell", "brace", "guard", "shield", "barrier", "defen"]) && !has(&["open the guard"]) {
            Move::Guard(if has(&["brace"]) { Accent::Gold } else { element.max_teal() })
        } else if has(&["drain", "siphon", "leech", "vitality"]) {
            Move::Drain
        } else if has(&["touch", "grasp"]) {
            Move::Touch(if element == Accent::Violet { Accent::Travel } else { element })
        } else if has(&["fear", "terror", "shriek", "howl", "roar"]) {
            Move::Fear
        } else if has(&["bolt", "lance", "spark", "missile", "ray", "blast", "magic"]) {
            Move::Cast(element)
        } else if has(&["falter", "curse", "hex", "weak", "slow", "charm", "sleep"]) {
            Move::Hex
        } else if has(&["rattling", "power", "smash", "crush", "heavy", "cleave", "slam"]) {
            Move::Heavy
        } else if has(&["dislocat", "hook", "twist", "trip", "sweep"]) {
            Move::Hook
        } else if has(&["open the guard", "bash", "parry", "batter"]) {
            Move::Bash
        } else if has(&["pressure", "push", "shove", "press", "charge"]) {
            Move::Shove
        } else if has(&["measured", "cut", "thrust", "lunge", "stab", "pierce"]) {
            Move::Thrust
        } else if has(&["second strike", "third strike"]) || strike.is_some_and(|s| s > 0) {
            Move::Combo(strike.unwrap_or(1) % 2 == 1 || has(&["second"]))
        } else if self_targeted {
            Move::Guard(Accent::Teal)
        } else {
            Move::Strike
        }
    }

    pub fn duration(self) -> f32 {
        match self {
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
        }
    }

    /// When in the move the blow lands or the spell leaves the hand.
    pub fn impact(self) -> f32 {
        match self {
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
        }
    }

    /// Whether the move is aimed at someone else.
    pub fn offensive(self) -> bool {
        !matches!(self, Move::Guard(_) | Move::Step | Move::Focus | Move::Mend)
    }
}

trait Teal {
    fn max_teal(self) -> Accent;
}
impl Teal for Accent {
    fn max_teal(self) -> Accent {
        if self == Accent::Violet {
            Accent::Teal
        } else {
            self
        }
    }
}

#[derive(Component)]
pub struct Anim {
    pub yaw: f32,
    pub target_yaw: Option<f32>,
    walk: f32,
    speed: f32,
    last: Vec3,
    time: f32,
    seed: f32,
    pub action: Option<(Move, f32)>,
    /// Seconds until a hit lands (negative while waiting), then its age.
    hurt: Option<f32>,
    dodge: Option<f32>,
    /// Death progress, 0 to 1. A corpse holds 1.
    pub dead: Option<f32>,
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
            last: at,
            time: 0.0,
            seed: rand(seed, 2) * 10.0,
            action: None,
            hurt: None,
            dodge: None,
            dead: None,
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
        self.action = Some((action, 0.0));
    }

    pub fn hit(&mut self, delay: f32) {
        self.hurt = Some(-delay);
    }

    pub fn miss(&mut self, delay: f32) {
        self.dodge = Some(-delay);
    }
}

// ---------------------------------------------------------------------------
// Building rigs.

struct Kit<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    library: &'a mut PropLibrary,
    meshes: &'a mut Assets<Mesh>,
    rig: Rig,
    key: String,
}

impl Kit<'_, '_, '_> {
    /// A joint at a rest offset from its parent, carrying one cached part.
    fn joint(&mut self, j: J, parent: Entity, at: Vec3, part: &str, build: impl FnOnce(&mut Parts)) -> Entity {
        let model = self.part(part, build);
        let e = self.commands.spawn((Transform::from_translation(at), Visibility::default())).id();
        self.commands.entity(e).with_children(|p| model.attach(p));
        self.commands.entity(parent).add_child(e);
        self.rig.joints[j as usize] = Some(e);
        self.rig.rest[j as usize] = at;
        e
    }

    fn part(&mut self, part: &str, build: impl FnOnce(&mut Parts)) -> Model {
        let soft = true;
        self.library.get(format!("creature/{}/{part}", self.key), self.meshes, || {
            let mut p = Parts::default();
            if soft {
                p.soften();
            }
            build(&mut p);
            p
        })
    }

    fn attach(&mut self, parent: Entity, at: Transform, part: &str, build: impl FnOnce(&mut Parts)) {
        let model = self.part(part, build);
        let e = self.commands.spawn((at, Visibility::default())).id();
        self.commands.entity(e).with_children(|p| model.attach(p));
        self.commands.entity(parent).add_child(e);
    }
}

/// Spawn a creature's rig under `parent` (the actor's body entity).
pub fn spawn(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    look: &Look,
) -> Rig {
    let mut kit = Kit {
        commands,
        library,
        meshes,
        rig: Rig {
            plan: look.plan,
            joints: [None; JOINTS],
            rest: [Vec3::ZERO; JOINTS],
            segments: Vec::new(),
            stride: look.stride,
            hunch: look.hunch,
            style: look.style,
            scale: look.scale,
        },
        key: look.key.clone(),
    };
    let lift = kit.commands.spawn((Transform::from_scale(Vec3::splat(look.scale)), Visibility::default())).id();
    kit.commands.entity(parent).add_child(lift);
    kit.rig.joints[J::Lift as usize] = Some(lift);
    match look.plan {
        Plan::Biped => biped(&mut kit, lift, look),
        Plan::Bird => bird(&mut kit, lift, look),
        Plan::Quadruped => quadruped(&mut kit, lift, look, false),
        Plan::Centaur => quadruped(&mut kit, lift, look, true),
        Plan::Serpent => serpent(&mut kit, lift, look),
        Plan::Floater => floater(&mut kit, lift, look),
    }
    if let Some((color, intensity)) = look.light() {
        let at = if look.plan == Plan::Floater { Vec3::Y * 1.1 } else { Vec3::Y * 1.2 };
        let light = kit
            .commands
            .spawn((
                PointLight {
                    color,
                    intensity,
                    range: 5.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(at),
            ))
            .id();
        kit.commands.entity(lift).add_child(light);
    }
    kit.rig
}

fn eyes(p: &mut Parts, accent: Option<Accent>, center: Vec3, spread: f32, size: f32) {
    if let Some(accent) = accent {
        p.accent = Some(accent);
        for side in [-1.0, 1.0] {
            p.glow.blob(center + Vec3::X * side * spread, Vec3::splat(size), 0, 0.0, Vec3::ONE);
        }
    } else {
        cartoon_eyes(p, center, spread, size, Vec3::new(0.36, 0.22, 0.11));
    }
}

/// The colour of this individual's hair, by kind.
fn hair_color(l: &Look) -> Vec3 {
    let t = l.v.hair_tone as usize;
    match l.style {
        Style::Vampire => [HAIRS[0], HAIRS[0], HAIRS[1], HAIRS[5], HAIRS[0], HAIRS[1]][t],
        Style::Hobgoblin => [DARK * 3.0, HAIRS[0], HAIRS[1], DARK * 3.0, HAIRS[1], HAIRS[0]][t],
        Style::Zombie => HAIRS[t] * 0.75 + Vec3::splat(0.05),
        _ => HAIRS[t],
    }
}

/// A sculpted lock of hair: one solid clump, broad at the root and tapering
/// to its tip, laid along `path`.
fn lock(b: &mut Builder, path: &[Vec3], width: f32, thick: f32, color: Vec3) {
    let n = path.len().max(2);
    let radii: Vec<Vec2> = (0..path.len())
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            Vec2::new((width * (1.0 - 0.8 * t * t)).max(0.004), (thick * (1.0 - 0.6 * t)).max(0.003))
        })
        .collect();
    // Broad side along the scalp: across the lock and around the head.
    let tangent = (path[1] - path[0]).normalize_or(Vec3::Y);
    let outward = (path[0] - Vec3::new(0.0, 0.17, -0.005)).normalize_or(Vec3::Y);
    let across = tangent.cross(outward).normalize_or(Vec3::X);
    b.tube_framed(path, &radii, 10, &|_, _, _| color, Some(across));
}

/// A plait hanging from `top`, ending in a little tuft.
fn braid(p: &mut Parts, top: Vec3, color: Vec3) {
    for k in 0..8 {
        let t = k as f32 / 7.0;
        let at = top + Vec3::new(if k % 2 == 0 { 0.008 } else { -0.008 }, -t * 0.26, -t * 0.03);
        p.matte.blob(at, Vec3::splat(0.032 - 0.012 * t), 1, 0.0, color * (if k % 2 == 0 { 1.0 } else { 0.92 }));
    }
    lock(&mut p.matte, &[top + Vec3::new(0.0, -0.27, -0.03), top + Vec3::new(0.0, -0.32, -0.035)], 0.02, 0.014, color);
}

/// A face of moderate, modelled features: skull and jaw, cheekbones, a nose
/// with a bridge and wings, lidded eyes, brows, lips and ears. Women's faces
/// are softer in the jaw, with finer brows, lashes and fuller lips.
fn human_face(p: &mut Parts, l: &Look, nose: bool) {
    let fem = l.v.woman;
    let face = l.skin;
    // The head is sculpted as one surface: cranium and jaw blended,
    // cheekbones and a brow over carved eye sockets, a nose that grows out of
    // the face with its wings, lips over a carved mouth line, a chin, ears.
    use crate::sdf::{Sculpt, Shape::*};
    let n = if fem { 0.85 } else { 1.0 };
    let jaw = if fem {
        Vec3::new(0.074, 0.06, 0.08)
    } else {
        [Vec3::new(0.08, 0.064, 0.082), Vec3::new(0.084, 0.066, 0.084), Vec3::new(0.09, 0.068, 0.086)][l.v.build as usize]
    };
    let mut head = Sculpt::default();
    head.add(Ellipsoid(Vec3::new(0.0, 0.172, -0.008), Vec3::new(0.098, 0.11, 0.106)), 0.0)
        .add(Ellipsoid(Vec3::new(0.0, 0.112, 0.022), jaw), 0.03)
        .add(Ellipsoid(Vec3::new(0.0, 0.07, 0.058), Vec3::new(0.034, 0.024, 0.03) * if fem { 0.85 } else { 1.0 }), 0.025)
        .add(Capsule(Vec3::new(0.0, 0.02, -0.01), Vec3::new(0.0, 0.1, -0.01), 0.05 * n, 0.052), 0.03);
    for side in [-1.0f32, 1.0] {
        // Cheekbones, and ears set back on the sides.
        head.add(Ellipsoid(Vec3::new(side * 0.054, 0.13, 0.055), Vec3::new(0.03, 0.02, 0.028)), 0.04)
            .add(Ellipsoid(Vec3::new(side * 0.098, 0.15, -0.005), Vec3::new(0.014, 0.03, 0.022)), 0.01);
    }
    // The brow, heavier on men.
    head.add(Ellipsoid(Vec3::new(0.0, 0.186, 0.078), Vec3::new(0.072, 0.018, 0.024) * if fem { 0.85 } else { 1.0 }), 0.02);
    if nose {
        head.add(Capsule(Vec3::new(0.0, 0.18, 0.094), Vec3::new(0.0, 0.135, 0.118), 0.009 * n, 0.014 * n), 0.012)
            .add(Ellipsoid(Vec3::new(0.0, 0.13, 0.117), Vec3::new(0.018, 0.015, 0.016) * n), 0.008);
        for side in [-1.0f32, 1.0] {
            head.add(Ellipsoid(Vec3::new(side * 0.014 * n, 0.126, 0.106), Vec3::splat(0.011 * n)), 0.006);
        }
    }
    // Lips, and the line of the mouth between them.
    let full = if fem { 1.25 } else { 1.0 };
    head.add(Ellipsoid(Vec3::new(0.0, 0.098, 0.093), Vec3::new(0.027, 0.007 * full, 0.01)), 0.01)
        .add(Ellipsoid(Vec3::new(0.0, 0.086, 0.091), Vec3::new(0.023, 0.008 * full, 0.01)), 0.01)
        ;
    // Each kind wears its own expression: adventurers half-smile, soldiers
    // and hobgoblins scowl, vampires smirk to one side, the dead hang slack.
    let (smile, scowl, smirk) = match l.style {
        Style::Hero | Style::Scout | Style::Caster | Style::Common | Style::Centaur => (0.006, -0.004, 0.0),
        Style::Hobgoblin | Style::Sentinel => (-0.004, 0.009, 0.0),
        Style::Vampire => (0.0, 0.005, 0.006),
        Style::Zombie => (-0.006, -0.006, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    // The mouth line curves up (or down) to the corners.
    head.carve(Capsule(Vec3::new(-0.025, 0.092 + smile, 0.098), Vec3::new(0.0, 0.091, 0.104), 0.002, 0.002), 0.003)
        .carve(Capsule(Vec3::new(0.0, 0.091, 0.104), Vec3::new(0.025, 0.092 + smile + smirk, 0.098), 0.002, 0.002), 0.003);
    if smile > 0.0 {
        // Cheeks lift with the smile.
        for side in [-1.0f32, 1.0] {
            head.add(Ellipsoid(Vec3::new(side * 0.036, 0.104, 0.082), Vec3::new(0.012, 0.009, 0.009)), 0.014);
        }
    }
    for side in [-1.0f32, 1.0] {
        // Eye sockets under the brow.
        head.carve(Ellipsoid(Vec3::new(side * 0.038, 0.16, 0.1), Vec3::new(0.023, 0.017, 0.016)), 0.01);
    }
    let lips = (face * 0.8).lerp(Vec3::new(0.72, 0.36, 0.36), if fem { 0.38 } else { 0.15 });
    let flush = face.lerp(Vec3::new(0.85, 0.45, 0.42), if fem { 0.14 } else { 0.05 });
    head.mesh(&mut p.matte, Vec3::new(-0.125, 0.0, -0.13), Vec3::new(0.125, 0.3, 0.14), 0.0055, &|at, _| {
        let lip = ((at.x / 0.026).powi(2) + ((at.y - 0.092) / 0.015).powi(2)) < 1.0 && at.z > 0.088;
        let cheek = (1.0 - Vec2::new(at.x.abs() - 0.055, at.y - 0.122).length() / 0.03).clamp(0.0, 1.0) * f32::from(u8::from(at.z > 0.05));
        let mottle = 1.0 + 0.025 * ((at.x * 173.0).sin() * (at.y * 151.0).cos() + (at.z * 197.0 + at.x * 61.0).sin() * 0.5);
        let warm = ((1.0 - Vec2::new(at.x, at.y - 0.13).length() / 0.02).clamp(0.0, 1.0) * f32::from(u8::from(at.z > 0.1))).max(
            (1.0 - (at.x.abs() - 0.098).abs() / 0.015).clamp(0.0, 1.0) * f32::from(u8::from((at.y - 0.15).abs() < 0.03)) * 0.6,
        );
        let base = if lip { lips } else { face.lerp(flush, cheek.max(warm * 0.6)) };
        base * mottle
    });
    let iris = [
        Vec3::new(0.36, 0.22, 0.11),
        Vec3::new(0.2, 0.12, 0.07),
        Vec3::new(0.45, 0.4, 0.18),
        Vec3::new(0.25, 0.45, 0.25),
        Vec3::new(0.25, 0.4, 0.62),
    ][((l.v.pattern + l.v.skin) % 5) as usize];
    match l.eyes {
        Some(_) => eyes(p, l.eyes, Vec3::new(0.0, 0.16, 0.088), 0.038, 0.015),
        None => soft_eyes(p, Vec3::new(0.0, 0.16, 0.088), 0.038, 0.0145, iris, face, fem),
    }
    // Arched brows, heavier on men, drawn down at the centre in a scowl.
    let brow_scowl = match l.style {
        Style::Hobgoblin | Style::Sentinel => 0.009,
        Style::Vampire => 0.005,
        Style::Zombie => -0.004,
        _ => -0.002,
    };
    let brow = hair_color(l) * 0.85;
    let (bw, bt) = if fem { (0.0055, 0.004) } else { (0.009, 0.006) };
    for side in [-1.0f32, 1.0] {
        tube(
            &mut p.matte,
            &[
                Vec3::new(side * 0.018, 0.187 - brow_scowl, 0.101),
                Vec3::new(side * 0.042, 0.197 + if fem { 0.003 } else { 0.0 } - brow_scowl * 0.3, 0.099),
                Vec3::new(side * 0.066, 0.189 + brow_scowl * 0.3, 0.09),
            ],
            &[(bw, bt), (bw, bt), (bw * 0.6, bt * 0.6)],
            brow,
        );
    }
}

/// Eyes set into the face: a white, an iris, a pupil and a glint, with an
/// upper lid that rests over the top and, for women, a line of lashes.
#[allow(clippy::too_many_arguments)]
fn soft_eyes(p: &mut Parts, center: Vec3, spread: f32, size: f32, iris: Vec3, face: Vec3, lashes: bool) {
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        p.matte.blob(at, Vec3::new(size * 1.4, size * 1.45, size * 0.8), 0, 0.0, Vec3::new(0.95, 0.94, 0.9));
        p.matte.blob(at + Vec3::new(0.0, -size * 0.05, size * 0.5), Vec3::new(size * 0.92, size * 0.98, size * 0.45), 0, 0.0, iris);
        p.matte.blob(at + Vec3::new(0.0, -size * 0.05, size * 0.75), Vec3::new(size * 0.48, size * 0.52, size * 0.25), 0, 0.0, DARK);
        p.matte.blob(at + Vec3::new(size * 0.3, size * 0.3, size * 0.85), Vec3::splat(size * 0.2), 0, 0.0, Vec3::splat(1.6));
        // The upper lid rests over the top of the eye.
        p.matte.blob(at + Vec3::new(0.0, size * 1.0, size * 0.18), Vec3::new(size * 1.6, size * 0.72, size * 0.85), 1, 0.0, face * 0.98);
        // The upper lid's dark edge gives every eye definition.
        tube(
            &mut p.matte,
            &[at + Vec3::new(-size * 1.35, size * 0.42, size * 0.62), at + Vec3::new(0.0, size * 0.66, size * 0.86), at + Vec3::new(size * 1.35, size * 0.42, size * 0.62)],
            &[(0.0022, 0.0022), (0.0028, 0.0028), (0.0022, 0.0022)],
            DARK * 2.0,
        );
        if lashes {
            tube(
                &mut p.matte,
                &[at + Vec3::new(-side * size * 1.3, size * 0.5, size * 0.72), at + Vec3::new(0.0, size * 0.7, size * 0.95), at + Vec3::new(side * size * 1.6, size * 0.75, size * 0.55)],
                &[(0.0025, 0.0025), (0.003, 0.003), (0.0015, 0.0015)],
                DARK,
            );
        }
    }
}

/// A short beard along the jaw to the chin, and a moustache.
fn beard(p: &mut Parts, color: Vec3) {
    let c = color * 0.95;
    for side in [-1.0f32, 1.0] {
        lock(&mut p.matte, &[Vec3::new(side * 0.088, 0.13, 0.0), Vec3::new(side * 0.075, 0.08, 0.055), Vec3::new(side * 0.03, 0.055, 0.095), Vec3::new(0.0, 0.05, 0.1)], 0.026, 0.02, c);
        lock(&mut p.matte, &[Vec3::new(side * 0.004, 0.1, 0.112), Vec3::new(side * 0.025, 0.097, 0.108), Vec3::new(side * 0.034, 0.086, 0.1)], 0.01, 0.008, c);
    }
    p.matte.blob(Vec3::new(0.0, 0.058, 0.088), Vec3::new(0.036, 0.022, 0.026), 2, 0.0, c);
}

/// A crown of sculpted clumps: broad locks radiating from the top of the
/// head down to the hairline, so the edge of the hair is a run of grouped
/// locks rather than the rim of a cap. `front`, `side` and `back` are the
/// heights the locks end at; `sweep` pushes them toward a parting.
#[allow(clippy::too_many_arguments)]
fn crown(p: &mut Parts, tone: &dyn Fn(u32) -> Vec3, front: f32, side: f32, back: f32, sweep: f32, width: f32, count: u32) {
    let skull = Vec3::new(0.0, 0.17, -0.005);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU;
        let facing = a.cos(); // 1 at the front, -1 at the back.
        let end_y = if facing >= 0.0 { side + (front - side) * facing } else { side + (back - side) * -facing };
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let root = Vec3::new(0.0, 0.292, -0.015) + dir * 0.025;
        // Points on the skull surface, a little proud of it.
        let on_skull = |y: f32, out: f32| {
            let h = ((y - skull.y) / 0.112).clamp(-1.0, 1.0);
            let r = (1.0 - h * h).max(0.0).sqrt();
            skull + Vec3::new(dir.x * 0.1 * r, y - skull.y, dir.z * 0.108 * r) * out
        };
        let mid_y = (0.29 + end_y) * 0.5 + 0.02;
        let mid = on_skull(mid_y, 1.16) + Vec3::X * sweep * 0.5;
        let end = on_skull(end_y, 1.1) + Vec3::X * sweep;
        lock(&mut p.matte, &[root, mid, end], width, 0.03, tone(k));
    }
}

/// Hair as sculpted clumps over a fitted cap, with volume where the style
/// carries it. Men: swept, cropped, tousled, slicked, tied back. Women: long,
/// ponytail, bun, braid, bob, all falling over the ears and temples.
fn hair(p: &mut Parts, l: &Look) {
    let c = hair_color(l);
    let v = l.v;
    let style = match l.style {
        Style::Vampire if !v.woman => 3,
        Style::Vampire => [0, 2, 0, 2, 4][v.hair as usize],
        Style::Zombie if !v.woman => 2,
        Style::Zombie => 0,
        _ => v.hair,
    };
    let seed = u64::from(v.hair_tone) * 31 + u64::from(style) + u64::from(v.skin) * 7;
    let tone = move |k: u32| c * (0.9 + 0.18 * rand(seed, k));
    let mass = |p: &mut Parts, at: Vec3, r: Vec3, color: Vec3| p.matte.blob(at, r, 2, 0.0, color);
    // The fitted cap under the clumps fills any gaps between them.
    mass(p, Vec3::new(0.0, 0.205, -0.02), Vec3::new(0.105, 0.094, 0.11), c * 0.9);
    if !v.woman {
        match style {
            // Swept up and over: a quiff above a crown of clumps.
            0 => {
                crown(p, &tone, 0.245, 0.19, 0.13, 0.0, 0.062, 11);
                lock(&mut p.matte, &[Vec3::new(-0.06, 0.27, 0.05), Vec3::new(-0.01, 0.315, 0.085), Vec3::new(0.05, 0.3, 0.1), Vec3::new(0.085, 0.255, 0.085)], 0.055, 0.04, tone(40));
            }
            // Cropped short and close.
            1 => crown(p, &tone, 0.25, 0.2, 0.15, 0.0, 0.05, 13),
            // Tousled: uneven clumps, tips lifting.
            2 => {
                crown(p, &tone, 0.235, 0.19, 0.12, 0.012, 0.06, 10);
                for k in 0..6 {
                    let a = k as f32 * 2.4;
                    let at = Vec3::new(a.cos() * 0.05, 0.29, a.sin() * 0.055);
                    let out = Vec3::new(a.cos(), 0.5, a.sin()).normalize();
                    lock(&mut p.matte, &[at, at + out * 0.05, at + out * 0.08 - Vec3::Y * 0.015], 0.045, 0.026, tone(20 + k));
                }
            }
            // Slicked straight back.
            3 => {
                crown(p, &tone, 0.24, 0.19, 0.12, 0.0, 0.07, 9);
                mass(p, Vec3::new(0.0, 0.25, -0.05), Vec3::new(0.104, 0.06, 0.1), c);
            }
            // Swept back and tied at the nape.
            _ => {
                crown(p, &tone, 0.245, 0.19, 0.14, 0.0, 0.065, 10);
                p.matte.torus(Vec3::new(0.0, 0.165, -0.13), Vec3::new(0.0, 0.3, -1.0).normalize(), 0.02, 0.007, 10, 4, c * 0.6);
                lock(&mut p.matte, &[Vec3::new(0.0, 0.165, -0.135), Vec3::new(0.0, 0.1, -0.165), Vec3::new(0.0, 0.03, -0.15)], 0.045, 0.04, c);
            }
        }
        return;
    }
    // Hers: a side-parted crown falling over the ears to the nape.
    crown(p, &tone, 0.235, 0.12, 0.09, 0.015, 0.068, 12);
    match style {
        // Long and loose over the shoulders, locks framing the face.
        0 => {
            lock(&mut p.matte, &[Vec3::new(0.0, 0.2, -0.1), Vec3::new(0.0, 0.1, -0.135), Vec3::new(0.0, -0.03, -0.13), Vec3::new(0.0, -0.16, -0.1)], 0.125, 0.05, c);
            for side in [-1.0f32, 1.0] {
                lock(&mut p.matte, &[Vec3::new(side * 0.095, 0.17, 0.03), Vec3::new(side * 0.11, 0.08, 0.02), Vec3::new(side * 0.105, -0.02, -0.01), Vec3::new(side * 0.09, -0.11, -0.03)], 0.05, 0.034, tone(40 + side as u32));
            }
        }
        // A high ponytail.
        1 => {
            p.matte.torus(Vec3::new(0.0, 0.215, -0.125), Vec3::new(0.0, 0.5, -1.0).normalize(), 0.024, 0.009, 10, 4, c * 0.6);
            lock(&mut p.matte, &[Vec3::new(0.0, 0.215, -0.135), Vec3::new(0.0, 0.17, -0.21), Vec3::new(0.0, 0.04, -0.21), Vec3::new(0.0, -0.07, -0.16)], 0.06, 0.05, c);
        }
        // A neat bun.
        2 => {
            mass(p, Vec3::new(0.0, 0.25, -0.115), Vec3::new(0.062, 0.056, 0.056), tone(6));
            p.matte.torus(Vec3::new(0.0, 0.25, -0.115), Vec3::new(0.0, 0.3, -1.0).normalize(), 0.055, 0.008, 12, 4, c * 0.75);
        }
        // A single long braid.
        3 => braid(p, Vec3::new(0.0, 0.13, -0.13), c),
        // A bob to the jaw.
        _ => {
            for side in [-1.0f32, 1.0] {
                lock(&mut p.matte, &[Vec3::new(side * 0.09, 0.25, 0.02), Vec3::new(side * 0.115, 0.15, 0.02), Vec3::new(side * 0.105, 0.07, 0.0)], 0.06, 0.04, tone(50 + side as u32));
            }
            mass(p, Vec3::new(0.0, 0.11, -0.08), Vec3::new(0.105, 0.085, 0.07), c);
        }
    }
}

/// A heraldic emblem laid flat on a surface: a sun, an eight-pointed star,
/// a crescent moon or a flame, by `kind`. `right` and `up` span the surface,
/// `out` faces away from it; `size` is the emblem's half-height; `field` is
/// the colour behind it, for cut-out shapes.
#[allow(clippy::too_many_arguments)]
fn emblem(b: &mut Builder, center: Vec3, right: Vec3, up: Vec3, out: Vec3, size: f32, color: Vec3, field: Vec3, kind: u8) {
    let at = |x: f32, y: f32, lift: f32| center + right * x * size + up * y * size + out * lift;
    let disc = |b: &mut Builder, x: f32, y: f32, r: f32, lift: f32, c: Vec3| {
        b.prism(at(x, y, lift - 0.003), at(x, y, lift + 0.003), r * size, r * size, 16, c, 0.0);
    };
    match kind % 4 {
        // A sun: a disc ringed with rays.
        0 => {
            disc(b, 0.0, 0.0, 0.38, 0.004, color);
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                let r = if k % 2 == 0 { 0.8 } else { 0.66 };
                b.prism(at(a.cos() * 0.42, a.sin() * 0.42, 0.004), at(a.cos() * r, a.sin() * r, 0.004), 0.1 * size, 0.0, 4, color, 0.0);
            }
        }
        // An eight-pointed star.
        1 => {
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                let r = if k % 2 == 0 { 0.85 } else { 0.55 };
                b.prism(at(0.0, 0.0, 0.004), at(a.cos() * r, a.sin() * r, 0.004), 0.16 * size, 0.0, 4, color, 0.0);
            }
            disc(b, 0.0, 0.0, 0.18, 0.006, field);
        }
        // A crescent moon, horns up.
        2 => {
            disc(b, 0.0, 0.0, 0.62, 0.004, color);
            disc(b, 0.0, 0.26, 0.56, 0.008, field);
        }
        // A flame: a tall tongue between two smaller ones.
        _ => {
            for (x, h, w) in [(0.0f32, 0.85f32, 0.3f32), (-0.3, 0.45, 0.2), (0.3, 0.45, 0.2)] {
                b.prism(at(x, -0.6, 0.004), at(x * 0.6, h, 0.004), w * size, 0.0, 8, color, 0.0);
            }
            disc(b, 0.0, -0.45, 0.32, 0.004, color);
        }
    }
}

/// A sculpted limb segment hanging down from its joint: a tapering core
/// from `r0` to `r1` over `len`, a muscle belly of `belly` a third of the way
/// down (pushed forward by `forward`), rounded at both ends so neighbouring
/// segments overlap without a seam.
fn limb(b: &mut Builder, len: f32, r0: f32, r1: f32, belly: f32, forward: f32, color: Vec3) {
    use crate::sdf::{Sculpt, Shape::*};
    let mut s = Sculpt::default();
    s.add(Capsule(Vec3::ZERO, Vec3::Y * -len, r0, r1), 0.0);
    if belly > 0.0 {
        s.add(Ellipsoid(Vec3::new(0.0, -len * 0.35, forward), Vec3::new(belly, len * 0.32, belly * 1.05)), 0.03);
    }
    let r = r0.max(r1).max(belly) + 0.02;
    s.mesh(b, Vec3::new(-r, -len - r, -r), Vec3::new(r, r, r), 0.007, &|at, _| color * Sculpt::weave(at));
}

/// A weathered stone slab: softly bevelled, its tone drifting across the
/// face, with moss settled on the upper surfaces.
fn stone(b: &mut Builder, center: Vec3, half: Vec3, color: Vec3, key: u64) {
    let moss = Vec3::new(0.38, 0.56, 0.17);
    let seed = key as f32 * 1.7;
    b.slab(center, half, Quat::IDENTITY, key, 0.06, &|dir, at| {
        let n = (at.x * 23.0 + seed).sin() * (at.z * 19.0 - seed).cos() + (at.y * 29.0 + at.x * 7.0).sin() * 0.6;
        let tone = color * (0.9 + 0.1 * (at.y * 9.0 + at.x * 5.0 + seed).sin());
        if (dir.y > 0.3 && n > 0.55) || (dir.y > -0.3 && n > 1.05) {
            moss
        } else {
            tone
        }
    });
}

/// A fist: a palm, four fingers curled under at the knuckles, and a thumb
/// wrapped across. `side` is +1 for the left hand, -1 for the right; `b`
/// scales it with the body.
fn fist(p: &mut Parts, at: Vec3, side: f32, b: f32, color: Vec3) {
    // The arm hangs with the palm toward the thigh (toward -side on x):
    // the knuckles run front to back, the fingers curl in toward the body,
    // and the thumb sits at the front, wrapping over them.
    let inward = -side;
    p.matte.blob(at, Vec3::new(0.03, 0.05, 0.044) * b, 2, 0.0, color);
    for k in 0..4 {
        let z = (1.5 - k as f32) * 0.019 * b;
        let knuckle = at + Vec3::new(side * 0.004, -0.036 * b, z);
        let r = (0.0115 - k as f32 * 0.0008) * b;
        tube(
            &mut p.matte,
            &[
                knuckle,
                knuckle + Vec3::new(inward * 0.018, -0.022, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, -0.012, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, 0.008, 0.0) * b,
            ],
            &[(r, r), (r, r), (r * 0.95, r * 0.95), (r * 0.8, r * 0.8)],
            color * (0.97 - k as f32 * 0.01),
        );
    }
    let base = at + Vec3::new(inward * 0.012, -0.004, 0.03) * b;
    tube(
        &mut p.matte,
        &[base, base + Vec3::new(inward * 0.014, -0.02, 0.016) * b, base + Vec3::new(inward * 0.03, -0.032, 0.004) * b],
        &[(0.014 * b, 0.014 * b), (0.012 * b, 0.012 * b), (0.01 * b, 0.01 * b)],
        color,
    );
}

/// Tufts of hair or fur: `count` tapered strands rising from around a
/// crown at `center`, swept along `sweep` and drooping at the tips. The
/// same strands make a fringe, a mane or a beard.
fn hair_tufts(p: &mut Parts, center: Vec3, radius: f32, count: u32, sweep: Vec3, color: Vec3, seed: u64) {
    let sweep = sweep.normalize_or(Vec3::Z);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU + rand(seed, k) * 0.5;
        let root = center + Vec3::new(a.cos() * radius * 0.7, rand(seed, k + 50) * 0.02, a.sin() * radius * 0.7);
        let out = (Vec3::new(a.cos(), 0.1, a.sin()) * 0.6 + sweep).normalize();
        let length = radius * (0.7 + rand(seed, k + 100) * 0.4);
        let w = radius * (0.36 + rand(seed, k + 150) * 0.1);
        // Each lock lies along the head, lifting a little before it falls.
        tube(
            &mut p.matte,
            &[root, root + out * length * 0.5 + Vec3::Y * length * 0.05, root + out * length - Vec3::Y * length * 0.45],
            &[(w, w * 0.6), (w * 0.8, w * 0.5), (0.003, 0.003)],
            color * (0.92 + rand(seed, k + 200) * 0.16),
        );
    }
}

/// A smooth tube of one colour through `path` with elliptical `radii`.
fn tube(b: &mut Builder, path: &[Vec3], radii: &[(f32, f32)], color: Vec3) {
    let radii: Vec<Vec2> = radii.iter().map(|(x, z)| Vec2::new(*x, *z)).collect();
    b.tube(path, &radii, 10, &|_, _, _| color);
}

/// A soft leaf-shaped feather from `root`, curving outward along `out`
/// and drooping as it goes; flat, broad in the middle, pointed at the tip.
fn feather(p: &mut Parts, root: Vec3, out: Vec3, length: f32, width: f32, color: Vec3) {
    let down = Vec3::NEG_Y;
    let path = [
        root,
        root + out * length * 0.3 + down * length * 0.25,
        root + out * length * 0.45 + down * length * 0.6,
        root + out * length * 0.5 + down * length,
    ];
    let w = width * 0.5;
    let radii = [Vec2::new(w * 0.4, 0.006), Vec2::new(w, 0.01), Vec2::new(w * 0.8, 0.008), Vec2::new(0.002, 0.002)];
    p.matte.tube(&path, &radii, 6, &|_, _, t| color * (1.0 - 0.18 * t));
}

/// Eyes with whites, a coloured iris, a pupil and a catchlight, set a
/// little forward of `center` on either side.
fn cartoon_eyes(p: &mut Parts, center: Vec3, spread: f32, size: f32, iris: Vec3) {
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        p.matte.blob(at, Vec3::new(size * 1.55, size * 1.75, size * 1.0), 0, 0.0, Vec3::new(0.95, 0.94, 0.9));
        p.matte.blob(at + Vec3::new(0.0, 0.0, size * 0.62), Vec3::new(size * 1.0, size * 1.1, size * 0.5), 0, 0.0, iris);
        p.matte.blob(at + Vec3::new(0.0, 0.0, size * 0.92), Vec3::new(size * 0.55, size * 0.62, size * 0.25), 0, 0.0, DARK);
        p.matte.blob(at + Vec3::new(size * 0.35, size * 0.42, size * 1.05), Vec3::splat(size * 0.22), 0, 0.0, Vec3::splat(1.6));
    }
}

/// Upright figures: people, the dead, constructs and the yeti.
///
/// The body is shaped rather than stacked: a chest that narrows to the
/// waist, shoulders, a skirt over the hips, tapered limbs with knees and
/// elbows, calves, hands with thumbs, cuffed boots. Clothing is layered in
/// separate colours so a figure reads at a glance even from above.
fn biped(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let style = l.style;
    let fem = l.v.woman;
    let b = match style {
        Style::Guardian => 1.4,
        Style::Yeti => 1.45,
        Style::Hobgoblin => 1.08,
        Style::Zombie => 0.92,
        Style::Vampire => 0.9,
        Style::Caster | Style::Scout => 0.92,
        _ => 1.0,
    } * [0.94, 1.0, 1.07][l.v.build as usize];
    // Women: narrower shoulders and waist, fuller hips.
    // Men taper from a broad chest to the waist; women have narrower
    // shoulders and waist and fuller hips.
    let (shoulders, waist, hips_w): (f32, f32, f32) = if fem { (0.9, 0.88, 1.1) } else { (1.04, 0.94, 1.0) };
    let arm_len = if style == Style::Yeti { 1.25 } else { 1.0 };
    let blocky = style == Style::Guardian;
    let furry = style == Style::Yeti;
    // The colour of each layer.
    let tunic = match style {
        Style::Guardian | Style::Yeti => l.cloth,
        Style::Sentinel => l.cloth,
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
        Style::Sentinel => IRON,
        Style::Guardian => l.skin,
        Style::Yeti => l.cloth,
        Style::Zombie => Vec3::new(0.3, 0.26, 0.2),
        Style::Vampire => DARK * 2.0,
        Style::Scout => Vec3::new(0.22, 0.2, 0.15),
        _ => l.cloth * 0.55 + LEATHER * 0.45,
    };
    let boot = match style {
        Style::Guardian => l.skin * 0.85,
        Style::Yeti => l.skin,
        Style::Sentinel => STEEL,
        Style::Zombie => l.skin * 0.8,
        Style::Vampire => DARK,
        _ => LEATHER * 0.75,
    };
    let sleeve = match style {
        Style::Guardian | Style::Hobgoblin => l.skin,
        Style::Yeti => l.cloth,
        Style::Sentinel => STEEL,
        Style::Zombie => l.skin,
        _ => tunic,
    };
    let hand_color = match style {
        Style::Yeti => l.skin,
        Style::Sentinel => STEEL,
        Style::Hero | Style::Scout => LEATHER,
        _ => l.skin,
    };
    let hips = kit.joint(J::Hips, lift, Vec3::Y * 0.95, "hips", |p| {
        if blocky {
            stone(&mut p.matte, Vec3::ZERO, Vec3::new(0.2, 0.1, 0.14), l.skin, 1);
            return;
        }
        p.matte.blob(Vec3::ZERO, Vec3::new(0.17 * hips_w, 0.12, 0.13) * b, 3, 0.05, pants);
        if furry {
            p.matte.blob(Vec3::new(0.0, -0.05, 0.0), Vec3::new(0.24, 0.16, 0.2), 6, 0.25, l.cloth);
            return;
        }
        // A skirt of the tunic over the hips breaks up the figure's column.
        let (skirt_color, skirt_len, metal) = match style {
            Style::Sentinel => (CREAM * 0.86, 0.26, false),
            Style::Caster => (l.cloth, 0.84, false),
            Style::Zombie => (tunic * 0.8, 0.2, false),
            Style::Vampire => (DARK, 0.3, false),
            _ => (tunic * 0.92, 0.22, false),
        };
        let skirt = if metal { &mut p.metal } else { &mut p.matte };
        // A sentinel's skirt hangs straight so the tabard panels lie over it.
        let flare = if style == Style::Sentinel { 0.185 } else { 0.235 + skirt_len * 0.1 };
        if metal {
            skirt.prism(Vec3::Y * 0.06, Vec3::Y * -skirt_len, 0.19 * b, flare * b, 10, skirt_color, 0.0);
        } else {
            // Cloth hangs in soft folds from the belt to the hem.
            use crate::sdf::{Sculpt, Shape::*};
            let (top, bottom) = (0.19 * b * hips_w, flare * b * hips_w);
            let mut cloth = Sculpt::default();
            cloth.add(Capsule(Vec3::Y * 0.04, Vec3::Y * (-skirt_len + 0.02), top, bottom), 0.0);
            let folds = 9;
            for k in 0..folds {
                let a = k as f32 / folds as f32 * TAU + 0.2;
                let dir = Vec3::new(a.sin(), 0.0, a.cos());
                let r = 0.018 + 0.012 * rand(u64::from(l.v.shade) + 7, k);
                cloth.add(Capsule(dir * top * 0.9 + Vec3::Y * 0.0, dir * bottom * 1.0 + Vec3::Y * (-skirt_len + 0.03), r * 0.5, r * 1.4), 0.03);
            }
            let w = bottom + 0.06;
            cloth.mesh(skirt, Vec3::new(-w, -skirt_len - 0.06, -w), Vec3::new(w, 0.1, w), if skirt_len > 0.5 { 0.011 } else { 0.0085 }, &|at, _| skirt_color * crate::sdf::Sculpt::weave(at));
        }
        if style == Style::Zombie {
            for k in 0..5 {
                let a = k as f32 * 1.3;
                let at = Vec3::new(a.cos() * 0.2, -0.2, a.sin() * 0.2);
                p.matte.prism(at, at + Vec3::new(a.cos() * 0.03, -0.1, a.sin() * 0.03), 0.04, 0.0, 3, tunic * 0.7, 0.0);
            }
        }
        if style == Style::Sentinel {
            // Long tabard panels front and back, gilt-edged, falling past the belt.
            for (z, lean) in [(0.19f32, -0.07f32), (-0.19, 0.07)] {
                let down = Vec3::new(0.0, 1.0, lean).normalize();
                let out = Vec3::Z * z.signum();
                p.metal.extrude(
                    &[Vec2::new(-0.11, 0.0), Vec2::new(0.11, 0.0), Vec2::new(0.125, -0.4), Vec2::new(-0.125, -0.4)],
                    Vec3::new(0.0, 0.05, z),
                    Vec3::X,
                    down,
                    0.012,
                    GILT,
                );
                p.matte.extrude(
                    &[Vec2::new(-0.097, 0.0), Vec2::new(0.097, 0.0), Vec2::new(0.11, -0.38), Vec2::new(-0.11, -0.38)],
                    Vec3::new(0.0, 0.05, z) + out * 0.01,
                    Vec3::X,
                    down,
                    0.012,
                    l.cloth,
                );
            }
        }
        if style == Style::Caster {
            p.accent = l.aura;
            p.glow.torus(Vec3::Y * -0.8, Vec3::Y, 0.31, 0.012, 20, 3, Vec3::ONE);
            p.matte.torus(Vec3::Y * -0.7, Vec3::Y, 0.3, 0.02, 20, 3, l.trim);
        }
        p.satin.torus(Vec3::Y * 0.07, Vec3::Y, 0.18 * b, 0.026, 14, 4, LEATHER * 0.8);
        p.metal.cuboid(Vec3::new(0.0, 0.07, 0.185 * b), Vec3::new(0.035, 0.03, 0.012), Quat::IDENTITY, Vec3::new(0.74, 0.5, 0.26));
        if matches!(style, Style::Hero | Style::Scout) {
            p.satin.cuboid(Vec3::new(0.15 * b, -0.02, 0.1), Vec3::new(0.035, 0.05, 0.025), Quat::from_rotation_y(0.5), LEATHER);
        }
    });
    let torso = kit.joint(J::Torso, hips, Vec3::Y * 0.06, "torso", |p| {
        match style {
            Style::Guardian => {
                stone(&mut p.matte, Vec3::Y * 0.3, Vec3::new(0.27, 0.27, 0.17), l.skin, 2);
                stone(&mut p.matte, Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.32, 0.1, 0.19), l.skin * 1.08, 3);
                stone(&mut p.matte, Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.2, 0.08, 0.13), l.skin * 0.9, 4);
                p.accent = Some(Accent::Teal);
                p.glow.torus(Vec3::new(0.0, 0.3, 0.172), Vec3::Z, 0.07, 0.012, 14, 3, Vec3::ONE);
                p.glow.cuboid(Vec3::new(0.0, 0.3, 0.172), Vec3::new(0.012, 0.06, 0.004), Quat::IDENTITY, Vec3::ONE);
                return;
            }
            Style::Yeti => {
                p.matte.blob(Vec3::Y * 0.28, Vec3::new(0.3, 0.34, 0.24), 9, 0.08, l.cloth);
                p.matte.blob(Vec3::new(0.0, 0.28, 0.12), Vec3::new(0.18, 0.2, 0.14), 4, 0.05, l.trim);
                p.matte.blob(Vec3::new(0.0, 0.5, -0.05), Vec3::new(0.26, 0.12, 0.2), 7, 0.1, l.cloth);
                // A mantle of long fur over the shoulders and down the chest.
                hair_tufts(p, Vec3::new(0.0, 0.56, 0.0), 0.24, 14, Vec3::new(0.0, -1.0, 0.0), l.cloth, 31);
                hair_tufts(p, Vec3::new(0.0, 0.42, 0.12), 0.14, 7, Vec3::new(0.0, -1.0, 0.6), l.cloth * 0.96, 37);
                return;
            }
            _ => {}
        }
        // The torso is sculpted as one form: waist, ribcage, a chest that
        // broadens into the shoulders, and deltoids that round over the top
        // of the arms, all blended so no seam shows. Fitted garments are the
        // same form grown by their thickness, so they follow the body.
        use crate::sdf::{Sculpt, Shape::*};
        let (sh, w) = (shoulders * b, waist * b);
        let form = |grow: f32| {
            let g = Vec3::splat(grow);
            let mut body = Sculpt::default();
            body.add(Ellipsoid(Vec3::new(0.0, 0.07, 0.0), Vec3::new(0.152 * w, 0.11, 0.112 * b) + g), 0.0)
                .add(Ellipsoid(Vec3::new(0.0, 0.29, 0.0), Vec3::new(0.198 * sh.max(w), 0.18, 0.12 * b) + g), 0.06)
                .add(Ellipsoid(Vec3::new(0.0, 0.45, -0.01), Vec3::new(0.205 * sh, 0.09, 0.106 * b) + g), 0.1)
                .add(Capsule(Vec3::new(0.0, 0.5, -0.005), Vec3::new(0.0, 0.6, -0.005), 0.07 * b + grow, 0.06 * b + grow), 0.04);
            for side in [-1.0f32, 1.0] {
                body.add(Ellipsoid(Vec3::new(side * 0.235 * sh, 0.47, 0.0), Vec3::new(0.066, 0.062, 0.072) * b + g), 0.06);
            }
            if fem && !matches!(style, Style::Sentinel) {
                body.add(Ellipsoid(Vec3::new(0.0, 0.34, 0.085 * b), Vec3::new(0.13, 0.065, 0.06) * b + g), 0.05);
            }
            body
        };
        {
            let skin = l.skin;
            let bare_shoulders = style == Style::Hobgoblin;
            form(0.0).mesh(&mut p.matte, Vec3::new(-0.38, -0.06, -0.2) * b.max(1.0), Vec3::new(0.38, 0.64, 0.22) * b.max(1.0), 0.0085, &move |at, _| {
                // A sleeveless shirt leaves the shoulders and neck bare.
                let bare = bare_shoulders && (at.x.abs() > 0.17 * b || at.y > 0.53);
                if bare { skin } else { tunic * crate::sdf::Sculpt::weave(at) }
            });
        }
        // A fitted outer garment: a jerkin, vest or coat over the tunic.
        let outer = match style {
            Style::Hero => Some((LEATHER, 0.43, true)),
            Style::Scout => Some((LEATHER * 0.85, 0.43, true)),
            Style::Hobgoblin => Some((LEATHER * 0.9, 0.43, true)),
            Style::Vampire => Some((Vec3::new(0.32, 0.05, 0.07), 0.47, false)),
            _ => None,
        };
        let shell = outer.map(|(color, top, sleeveless)| {
            let mut shell = form(0.014);
            shell.carve(Beyond(Vec3::Y, top), 0.01).carve(Beyond(-Vec3::Y, 0.0), 0.0);
            if sleeveless {
                for side in [-1.0f32, 1.0] {
                    shell.carve(Beyond(Vec3::X * side, 0.185 * sh), 0.012);
                }
            }
            let c = color;
            shell.mesh(&mut p.satin, Vec3::new(-0.36, -0.03, -0.2) * b.max(1.0), Vec3::new(0.36, top + 0.03, 0.22) * b.max(1.0), 0.0075, &move |at, n| {
                // Seams darken the front opening and the edges.
                let seam = if at.x.abs() < 0.006 && n.z > 0.5 { 0.7 } else { 1.0 };
                c * seam * (1.0 + 0.03 * (at.y * 57.0 + at.x * 31.0).sin())
            });
            shell
        });
        // Where something sits on the front (or back) of the outer garment.
        let on = |x: f32, y: f32, front: bool| -> Vec3 {
            let dir = if front { -Vec3::Z } else { Vec3::Z };
            match &shell {
                Some(s) => s.onto(Vec3::new(x, y, -dir.z * 0.3), dir),
                None => form(0.0).onto(Vec3::new(x, y, -dir.z * 0.3), dir),
            }
        };
        p.matte.torus(Vec3::Y * 0.555, Vec3::Y, 0.07, 0.022, 12, 4, l.trim);
        // A soft cowl or scarf gathered round the neck and draped over the shoulders.
        let cowl = match style {
            Style::Hero => Some((tunic * 0.75, false)),
            Style::Caster => Some((l.cloth * 0.85, false)),
            Style::Scout => Some((Vec3::new(0.3, 0.42, 0.22), true)),
            _ => None,
        };
        if let Some((color, tail)) = cowl {
            // A soft cowl of cloth: two loose rolls gathered round the neck,
            // the upper snug, the lower settling over the shoulders and
            // fuller at the back where the folded hood rests.
            let mut c = Sculpt::default();
            for (y, rx, rz, thick, k) in [(0.535f32, 0.105f32, 0.1f32, 0.042f32, 0.035f32), (0.49, 0.15, 0.125, 0.038, 0.04)] {
                let ring: Vec<Vec3> = (0..=16)
                    .map(|i| {
                        let a = i as f32 / 16.0 * TAU;
                        let (sx, cz) = (a.sin(), a.cos());
                        // Dipping at the sides onto the shoulders, riding up at the throat.
                        Vec3::new(sx * rx * b, y - 0.02 * sx.abs() + 0.01 * cz.max(0.0), cz * rz * b - 0.012)
                    })
                    .collect();
                for (i, pair) in ring.windows(2).enumerate() {
                    let a = i as f32 / 16.0 * TAU;
                    // Thicker behind, thinner under the chin.
                    let t = thick * (1.0 - 0.25 * a.cos()) * b;
                    c.add(Capsule(pair[0], pair[1], t, t), k);
                }
            }
            c.add(Ellipsoid(Vec3::new(0.0, 0.5, -0.12 * b), Vec3::new(0.12, 0.06, 0.05) * b), 0.05);
            // Leave the neck through the middle and keep the roll off the chest.
            c.carve(Capsule(Vec3::Y * 0.45, Vec3::Y * 0.7, 0.062 * b, 0.062 * b), 0.01);
            if tail {
                c.add(Capsule(Vec3::new(0.06, 0.5, 0.12), Vec3::new(0.07, 0.26, 0.15), 0.035, 0.03), 0.03);
            }
            c.mesh(&mut p.matte, Vec3::new(-0.26, 0.2, -0.2) * b.max(1.0), Vec3::new(0.26, 0.64, 0.22) * b.max(1.0), 0.0075, &|at, _| color * crate::sdf::Sculpt::weave(at));
        }
        if fem && style == Style::Vampire {
            let (front, color, satin) = match style {
                Style::Hero => (0.146, LEATHER, true),
                Style::Scout => (0.18, LEATHER * 0.85, true),
                Style::Hobgoblin => (0.144, LEATHER * 0.9, true),
                Style::Sentinel => (0.19, l.cloth, false),
                Style::Vampire => (0.145, Vec3::new(0.32, 0.05, 0.07), false),
                _ => (0.13 * b, tunic, false),
            };
            let m = if satin { &mut p.satin } else { &mut p.matte };
            m.blob(Vec3::new(0.0, 0.34, front - 0.045), Vec3::new(0.12, 0.06, 0.05) * b, 2, 0.0, color);
        }
        match style {
            Style::Hero => {
                // Lacing up the front of the jerkin and a strap across it.
                for k in 0..4 {
                    let at = on(0.0, 0.1 + k as f32 * 0.08, true);
                    p.matte.cuboid(at + Vec3::Z * 0.004, Vec3::new(0.026, 0.005, 0.005), Quat::from_rotation_z(0.6), BONE);
                }
                let strap: Vec<Vec3> = [(0.17, 0.43), (0.07, 0.33), (-0.06, 0.2), (-0.16, 0.07)].iter().map(|&(x, y)| on(x * b, y, true) + Vec3::Z * 0.006).collect();
                tube(&mut p.satin, &strap, &[(0.024, 0.006); 4], LEATHER * 0.7);
                p.metal.blob(on(0.0, 0.3, true) + Vec3::Z * 0.008, Vec3::new(0.02, 0.02, 0.008), 0, 0.0, Vec3::new(0.74, 0.5, 0.26));
            }
            Style::Sentinel => {
                p.metal.blob(Vec3::Y * 0.34, Vec3::new(0.225, 0.19, 0.152), 2, 0.0, STEEL);
                p.metal.prism(Vec3::Y * 0.04, Vec3::Y * 0.22, 0.155, 0.175, 10, STEEL * 0.9, 0.0);
                // A large cloth tabard over the breastplate: narrow side to
                // side, so its flanks sit inside the armour and only the
                // front and back show, following the chest down to the belt.
                tube(
                    &mut p.matte,
                    &[Vec3::Y * 0.0, Vec3::Y * 0.14, Vec3::Y * 0.3, Vec3::Y * 0.44, Vec3::Y * 0.52],
                    &[(0.15, 0.18), (0.155, 0.182), (0.17, 0.19), (0.16, 0.17), (0.1, 0.12)],
                    l.cloth,
                );
                // A gilt hem where the tabard meets the belt, and a large cream cross.
                p.metal.torus(Vec3::Y * 0.01, Vec3::Y, 0.155, 0.008, 22, 3, GILT);
                for z in [0.192f32, -0.192] {
                    emblem(&mut p.matte, Vec3::new(0.0, 0.29, z), Vec3::X * z.signum(), Vec3::Y, Vec3::Z * z.signum(), 0.1, CREAM, l.cloth, l.v.pattern);
                }
                p.metal.torus(Vec3::Y * 0.05, Vec3::Y, 0.168, 0.012, 20, 4, Vec3::new(0.8, 0.62, 0.3));
                p.metal.torus(Vec3::Y * 0.22, Vec3::Y, 0.178, 0.01, 20, 4, Vec3::new(0.8, 0.62, 0.3));
                for side in [-1.0f32, 1.0] {
                    // Three overlapping lames, each rimmed in gilt.
                    for (k, (y, r)) in [(0.51, 0.118), (0.465, 0.11), (0.425, 0.1)].into_iter().enumerate() {
                        let at = Vec3::new(side * (0.245 + k as f32 * 0.018), y, 0.0);
                        p.metal.blob(at, Vec3::new(r, 0.05, r * 1.02), 3, 0.0, STEEL * (1.0 - k as f32 * 0.05));
                        if k == 2 {
                            p.metal.torus(at - Vec3::Y * 0.02, Vec3::Y, r * 0.93, 0.01, 18, 3, Vec3::new(0.8, 0.62, 0.3));
                        }
                    }
                }
            }
            Style::Caster => {
                p.matte.cuboid(Vec3::new(0.0, 0.25, 0.14), Vec3::new(0.04, 0.24, 0.01), Quat::IDENTITY, l.trim);
                p.matte.torus(Vec3::Y * 0.05, Vec3::Y, 0.155, 0.024, 20, 6, l.trim * 0.8);
                p.matte.blob(Vec3::new(0.06, 0.05, 0.15), Vec3::new(0.035, 0.03, 0.02), 1, 0.0, l.trim * 0.8);
                for (dx, end) in [(0.05f32, 0.32f32), (0.085, 0.26)] {
                    tube(
                        &mut p.matte,
                        &[Vec3::new(dx, 0.04, 0.155), Vec3::new(dx + 0.01, -0.1, 0.17), Vec3::new(dx + 0.015, -end, 0.175)],
                        &[(0.022, 0.006), (0.024, 0.006), (0.026, 0.006)],
                        l.trim * 0.8,
                    );
                }
                p.matte.blob(Vec3::new(0.0, 0.52, -0.08), Vec3::new(0.2, 0.08, 0.1), 2, 0.0, l.cloth * 0.9);
            }
            Style::Scout => {
                // A baldric across the jerkin.
                let strap: Vec<Vec3> = [(-0.17, 0.43), (-0.06, 0.32), (0.07, 0.19), (0.17, 0.07)].iter().map(|&(x, y)| on(x * b, y, true) + Vec3::Z * 0.006).collect();
                tube(&mut p.satin, &strap, &[(0.026, 0.006); 4], LEATHER);
            }
            Style::Hobgoblin => {
                // A riveted leather vest on shoulder straps.
                for side in [-1.0f32, 1.0] {
                    let (f, k) = (on(side * 0.11, 0.41, true), on(side * 0.11, 0.41, false));
                    tube(
                        &mut p.satin,
                        &[f, Vec3::new(side * 0.13, 0.53, 0.05), Vec3::new(side * 0.13, 0.53, -0.06), k],
                        &[(0.03, 0.01); 4],
                        LEATHER * 0.75,
                    );
                }
                for k in 0..3 {
                    p.metal.blob(on(-0.1 + k as f32 * 0.1, 0.16, true), Vec3::splat(0.012), 0, 0.0, IRON);
                }
            }
            Style::Zombie => {
                p.matte.blob(Vec3::new(0.06, 0.25, 0.14), Vec3::new(0.08, 0.1, 0.04), 7, 0.4, l.skin * 0.85);
                for k in 0..3 {
                    p.matte.cuboid(Vec3::new(0.06, 0.2 + k as f32 * 0.04, 0.15), Vec3::new(0.045, 0.006, 0.006), Quat::IDENTITY, BONE * 0.8);
                }
                p.matte.blob(Vec3::new(-0.12, 0.38, 0.13), Vec3::new(0.05, 0.06, 0.03), 8, 0.4, Vec3::new(0.35, 0.12, 0.1));
            }
            Style::Vampire => {
                // A stand-up collar framing the head from behind.
                // Black shoulder pads.
                for side in [-1.0f32, 1.0] {
                    p.matte.blob(Vec3::new(side * 0.2 * b, 0.47, 0.0), Vec3::new(0.085, 0.045, 0.09), 2, 0.0, DARK * 1.6);
                }
                // A high collar, red-lined, rising behind the head and folding open at the front.
                tube(
                    &mut p.matte,
                    &[Vec3::new(0.0, 0.5, -0.05), Vec3::new(0.0, 0.58, -0.08), Vec3::new(0.0, 0.67, -0.11)],
                    &[(0.11, 0.085), (0.13, 0.095), (0.155, 0.105)],
                    l.trim,
                );
                // A narrow white cravat and two rows of gold buttons.
                let cravat: Vec<Vec3> = [0.46f32, 0.36, 0.26].iter().map(|&y| on(0.0, y, true) + Vec3::Z * 0.005).collect();
                tube(&mut p.matte, &cravat, &[(0.024, 0.008), (0.02, 0.008), (0.012, 0.006)], Vec3::new(0.88, 0.85, 0.8));
                for side in [-1.0f32, 1.0] {
                    for k in 0..3 {
                        p.metal.blob(on(side * 0.055, 0.13 + k as f32 * 0.08, true), Vec3::splat(0.011), 0, 0.0, Vec3::new(0.9, 0.75, 0.35));
                    }
                }
            }
            _ => {}
        }
    });
    kit.joint(J::Head, torso, Vec3::Y * 0.6, "head", |p| {
        let neck = if furry { l.cloth } else { l.skin };
        let slim = if fem { 0.85 } else { 1.0 };
        p.matte.prism(Vec3::Y * -0.03, Vec3::Y * 0.08, 0.06 * b * slim, 0.052 * b * slim, 7, neck, 0.0);
        match style {
            Style::Guardian => {
                stone(&mut p.matte, Vec3::Y * 0.15, Vec3::new(0.13, 0.12, 0.13), l.skin, 5);
                stone(&mut p.matte, Vec3::new(0.0, 0.2, 0.115), Vec3::new(0.12, 0.03, 0.03), l.skin * 0.8, 6);
                stone(&mut p.matte, Vec3::new(0.0, 0.07, 0.11), Vec3::new(0.08, 0.025, 0.03), l.skin * 0.75, 7);
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.13), 0.05, 0.022);
                // A carved, contented smile.
                p.matte.cuboid(Vec3::new(0.0, 0.085, 0.132), Vec3::new(0.04, 0.007, 0.006), Quat::IDENTITY, DARK * 2.0);
                for side in [-1.0f32, 1.0] {
                    p.matte.cuboid(Vec3::new(side * 0.045, 0.092, 0.132), Vec3::new(0.012, 0.006, 0.006), Quat::from_rotation_z(side * 0.6), DARK * 2.0);
                }
            }
            Style::Yeti => {
                p.matte.blob(Vec3::new(0.0, 0.14, 0.0), Vec3::new(0.15, 0.15, 0.14), 6, 0.06, l.cloth);
                hair_tufts(p, Vec3::new(0.0, 0.24, -0.02), 0.12, 9, Vec3::new(0.0, -0.4, -1.0), l.cloth, 41);
                // An ape's dark face: broad brow, flat nose, heavy muzzle.
                p.matte.blob(Vec3::new(0.0, 0.12, 0.09), Vec3::new(0.1, 0.1, 0.07), 2, 0.0, l.skin);
                p.matte.blob(Vec3::new(0.0, 0.175, 0.12), Vec3::new(0.09, 0.025, 0.03), 3, 0.0, l.skin * 0.9);
                p.matte.blob(Vec3::new(0.0, 0.06, 0.12), Vec3::new(0.06, 0.035, 0.04), 2, 0.0, l.skin * 0.8);
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.15), 0.035, 0.016);
                for side in [-1.0f32, 1.0] {
                    p.matte.prism(Vec3::new(side * 0.1, 0.24, 0.0), Vec3::new(side * 0.18, 0.34, -0.06), 0.035, 0.0, 6, BONE, 0.0);
                    p.matte.prism(Vec3::new(side * 0.035, 0.04, 0.13), Vec3::new(side * 0.04, 0.0, 0.14), 0.012, 0.0, 4, BONE, 0.0);
                }
            }
            _ => {
                // A slightly large head reads well from above.
                let size = if fem { 1.1 } else { 1.14 };
                p.set_transform(Affine3A::from_translation(Vec3::Y * 0.08) * Affine3A::from_scale(Vec3::splat(size)) * Affine3A::from_translation(Vec3::Y * -0.08));
                human_face(p, &l, style != Style::Hobgoblin);
                let hood_down = matches!(style, Style::Caster | Style::Scout) && l.v.pattern >= 2;
                let hooded = matches!(style, Style::Caster | Style::Scout | Style::Sentinel) && !hood_down;
                if !hooded && style != Style::Hobgoblin {
                    hair(p, &l);
                }
                if l.v.beard && matches!(style, Style::Hero | Style::Caster | Style::Scout | Style::Common | Style::Zombie) {
                    beard(p, hair_color(&l));
                }
                match style {
                    Style::Hero => {}
                    Style::Sentinel => {
                        tube(
                            &mut p.metal,
                            &[Vec3::Y * 0.04, Vec3::Y * 0.14, Vec3::Y * 0.26, Vec3::Y * 0.31],
                            &[(0.128, 0.132), (0.136, 0.14), (0.13, 0.134), (0.11, 0.112)],
                            STEEL,
                        );
                        p.metal.blob(Vec3::Y * 0.3, Vec3::new(0.11, 0.05, 0.112), 0, 0.0, STEEL);
                        // The visor: a dark slit, a raised ridge down the face, breathing holes.
                        p.matte.cuboid(Vec3::new(0.0, 0.17, 0.128), Vec3::new(0.095, 0.011, 0.02), Quat::IDENTITY, DARK);
                        p.metal.cuboid(Vec3::new(0.0, 0.12, 0.138), Vec3::new(0.01, 0.06, 0.012), Quat::IDENTITY, STEEL * 1.1);
                        for side in [-1.0f32, 1.0] {
                            for k in 0..3 {
                                p.matte.blob(Vec3::new(side * 0.05, 0.085 + k as f32 * 0.02, 0.124), Vec3::splat(0.006), 0, 0.0, DARK);
                            }
                        }
                        p.metal.torus(Vec3::Y * 0.05, Vec3::Y, 0.13, 0.014, 18, 4, Vec3::new(0.8, 0.62, 0.3));
                        if fem {
                            braid(p, Vec3::new(0.0, 0.05, -0.12), hair_color(&l));
                        }
                        // A plume of the house colour sweeping back from the crest.
                        for k in 0..6 {
                            let spread = (k as f32 - 2.5) * 0.012;
                            let base = Vec3::new(spread, 0.33, 0.03);
                            tube(
                                &mut p.matte,
                                &[base, base + Vec3::new(0.0, 0.09, -0.04), base + Vec3::new(spread, 0.1, -0.16), base + Vec3::new(spread * 2.0, 0.02, -0.26)],
                                &[(0.016, 0.022), (0.018, 0.026), (0.013, 0.018), (0.002, 0.002)],
                                l.cloth * (0.9 + 0.04 * k as f32),
                            );
                        }
                    }
                    Style::Caster | Style::Scout if hood_down => {
                        let hood = if style == Style::Caster { l.cloth } else { Vec3::new(0.3, 0.42, 0.22) };
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut cowl = Sculpt::default();
                            cowl.add(Ellipsoid(Vec3::new(0.0, 0.0, -0.12), Vec3::new(0.12, 0.07, 0.06)), 0.0);
                            for side in [-1.0f32, 1.0] {
                                cowl.add(Ellipsoid(Vec3::new(side * 0.1, -0.02, -0.06), Vec3::new(0.05, 0.05, 0.07)), 0.05);
                            }
                            // The hollow of the hood, open toward the neck.
                            cowl.carve(Ellipsoid(Vec3::new(0.0, 0.03, -0.1), Vec3::new(0.085, 0.045, 0.04)), 0.02);
                            cowl.mesh(&mut p.matte, Vec3::new(-0.2, -0.1, -0.22), Vec3::new(0.2, 0.1, 0.02), 0.007, &|at, _| hood * 0.95 * crate::sdf::Sculpt::weave(at));
                        }
                    }
                    Style::Caster | Style::Scout => {
                        let hood = if style == Style::Caster { l.cloth } else { Vec3::new(0.3, 0.42, 0.22) };
                        // A deep hood: the shell rounds over the head, a thick
                        // rolled rim frames the face, and the cloth drapes down
                        // the back of the neck onto the shoulders.
                        // One sculpted piece of cloth: a shell rounding over the
                        // head, the face opening cut cleanly with only a soft
                        // thickness at its edge, the cloth falling behind the
                        // neck onto the shoulders.
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut h = Sculpt::default();
                            h.add(Ellipsoid(Vec3::new(0.0, 0.195, -0.035), Vec3::new(0.138, 0.165, 0.145)), 0.0)
                                .add(Ellipsoid(Vec3::new(0.0, 0.07, -0.11), Vec3::new(0.12, 0.13, 0.07)), 0.05)
                                .add(Ellipsoid(Vec3::new(0.0, -0.03, -0.09), Vec3::new(0.16, 0.05, 0.09)), 0.05)
                                // The face opening, tilted to follow the face.
                                .carve(Ellipsoid(Vec3::new(0.0, 0.14, 0.13), Vec3::new(0.088, 0.13, 0.11)), 0.012)
                                // The underside around the neck and the front of the shoulders.
                                .carve(Ellipsoid(Vec3::new(0.0, -0.06, 0.1), Vec3::new(0.2, 0.08, 0.14)), 0.02);
                            h.mesh(&mut p.matte, Vec3::new(-0.2, -0.1, -0.22), Vec3::new(0.2, 0.38, 0.14), 0.0065, &|_at, n| {
                                // A slightly darker edge where the cloth turns inward.
                                (if n.z > 0.6 { hood * 0.88 } else { hood }) * crate::sdf::Sculpt::weave(_at)
                            });
                        }
                        if fem {
                            // Locks of hair escape the hood to frame her face.
                            let c = hair_color(&l);
                            for side in [-1.0f32, 1.0] {
                                lock(
                                    &mut p.matte,
                                    &[Vec3::new(side * 0.075, 0.2, 0.07), Vec3::new(side * 0.088, 0.13, 0.07), Vec3::new(side * 0.08, 0.05, 0.06)],
                                    0.026,
                                    0.016,
                                    c,
                                );
                            }
                        }
                        // Some hoods end in a long soft point that flops back.
                        let pointed = style == Style::Scout || l.aura == Some(Accent::Ember);
                        if pointed {
                            tube(
                                &mut p.matte,
                                &[Vec3::new(0.0, 0.3, -0.06), Vec3::new(0.0, 0.37, -0.14), Vec3::new(0.0, 0.33, -0.24), Vec3::new(0.0, 0.22, -0.27)],
                                &[(0.075, 0.06), (0.05, 0.045), (0.03, 0.028), (0.004, 0.004)],
                                hood * 0.95,
                            );
                        }
                    }
                    Style::Hobgoblin => {
                        for side in [-1.0f32, 1.0] {
                            p.matte.prism(Vec3::new(side * 0.1, 0.17, 0.0), Vec3::new(side * 0.26, 0.23, -0.05), 0.035, 0.0, 5, l.skin, 0.0);
                            p.matte.prism(Vec3::new(side * 0.03, 0.08, 0.1), Vec3::new(side * 0.035, 0.12, 0.11), 0.01, 0.0, 3, BONE, 0.0);
                        }
                        // A big ruddy nose and a stiff dark mohawk, or, for her, a wild topknot.
                        p.matte.blob(Vec3::new(0.0, 0.125, 0.115), Vec3::new(0.04, 0.038, 0.045), 2, 0.0, Vec3::new(0.78, 0.36, 0.3));
                        if fem {
                            let c = hair_color(&l);
                            p.matte.blob(Vec3::new(0.0, 0.28, -0.04), Vec3::new(0.035, 0.03, 0.035), 1, 0.0, c);
                            for k in 0..5 {
                                let a = k as f32 * 1.25;
                                lock(&mut p.matte, &[Vec3::new(0.0, 0.29, -0.04), Vec3::new(a.cos() * 0.05, 0.33, -0.05 + a.sin() * 0.05), Vec3::new(a.cos() * 0.1, 0.27, -0.06 + a.sin() * 0.1)], 0.024, 0.014, c);
                            }
                        }
                        for k in 0..(if fem { 0 } else { 6 }) {
                            let z = 0.06 - k as f32 * 0.045;
                            let root = Vec3::new(0.0, 0.25 - (k as f32 - 2.0).powi(2) * 0.004, z);
                            tube(&mut p.matte, &[root, root + Vec3::new(0.0, 0.06, -0.03)], &[(0.012, 0.03), (0.002, 0.004)], DARK * 3.0);
                        }
                    }
                    Style::Zombie => {
                        // The slack, open mouth.
                        p.matte.blob(Vec3::new(0.004, 0.089, 0.107), Vec3::new(0.018, 0.009, 0.006), 0, 0.0, DARK * 2.0);
                        p.matte.blob(Vec3::new(-0.06, 0.2, 0.08), Vec3::new(0.025, 0.02, 0.015), 2, 0.3, Vec3::new(0.35, 0.12, 0.1));
                    }
                    Style::Vampire => {
                        p.matte.prism(Vec3::new(0.0, 0.25, 0.07), Vec3::new(0.0, 0.18, 0.11), 0.03, 0.0, 4, DARK, 0.0);
                        for side in [-1.0f32, 1.0] {
                            p.matte.prism(Vec3::new(side * 0.018, 0.085, 0.1), Vec3::new(side * 0.018, 0.06, 0.1), 0.006, 0.0, 3, BONE, 0.0);
                        }
                    }
                    _ => {}
                }
            }
        }
    });
    for (side, arm, fore, hand) in [(1.0f32, J::ArmL, J::ForeL, J::HandL), (-1.0, J::ArmR, J::ForeR, J::HandR)] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let shoulder = Vec3::new(side * 0.25 * b * shoulders, 0.5, 0.0);
        let metal = style == Style::Sentinel;
        let upper = kit.joint(arm, torso, shoulder, &format!("arm{tag}"), |p| {
            let m = if metal { &mut p.metal } else { &mut p.matte };
            if blocky {
                stone(m, Vec3::Y * -0.16, Vec3::new(0.085, 0.17, 0.085), sleeve, 8);
                return;
            }
            if metal || furry {
                m.blob(Vec3::new(-side * 0.012, -0.02, 0.0), Vec3::new(0.062, 0.06, 0.062) * b, 2, 0.0, sleeve);
                tube(
                    m,
                    &[Vec3::ZERO, Vec3::new(0.0, -0.1 * arm_len, 0.006), Vec3::Y * -0.2 * arm_len, Vec3::Y * -0.3 * arm_len],
                    &[(0.06 * b, 0.06 * b), (0.064 * b, 0.068 * b), (0.058 * b, 0.06 * b), (0.05 * b, 0.05 * b)],
                    sleeve,
                );
            } else {
                // Shoulder to elbow in one sculpted piece, with the biceps.
                limb(m, 0.3 * arm_len, 0.056 * b, 0.046 * b, 0.05 * b, 0.006, sleeve);
            }
            if furry {
                m.blob(Vec3::Y * -0.12, Vec3::new(0.1, 0.16, 0.1), 3, 0.1, sleeve);
                // Shaggy fur spilling off the shoulder.
                hair_tufts(p, Vec3::new(0.0, 0.02, 0.0), 0.1, 8, Vec3::new(side, -1.2, 0.0), l.cloth * 0.95, 29 + side as u64);
            }
        });
        let lower = kit.joint(fore, upper, Vec3::Y * -0.3 * arm_len, &format!("fore{tag}"), |p| {
            if blocky {
                stone(&mut p.matte, Vec3::Y * -0.15, Vec3::new(0.075, 0.15, 0.075), l.skin * 0.9, 9);
                stone(&mut p.matte, Vec3::Y * -0.34, Vec3::new(0.105, 0.085, 0.105), l.skin, 10);
                return;
            }
            if metal {
                // An elbow cop with a fan.
                p.metal.blob(Vec3::new(0.0, 0.0, -0.01), Vec3::new(0.07, 0.065, 0.07), 2, 0.0, STEEL * 1.05);
                p.metal.blob(Vec3::new(side * 0.055, 0.0, -0.01), Vec3::new(0.012, 0.05, 0.045), 2, 0.0, STEEL);
            }
            let m = if metal { &mut p.metal } else { &mut p.matte };
            if metal {
                m.blob(Vec3::ZERO, Vec3::splat(0.058 * b), 2, 0.0, sleeve);
                tube(
                    m,
                    &[Vec3::ZERO, Vec3::new(0.0, -0.07 * arm_len, 0.004), Vec3::Y * -0.17 * arm_len, Vec3::Y * -0.26 * arm_len],
                    &[(0.05 * b, 0.05 * b), (0.06 * b, 0.056 * b), (0.05 * b, 0.046 * b), (0.04 * b, 0.036 * b)],
                    sleeve,
                );
            } else {
                // Elbow to wrist: the forearm swells below the elbow and tapers.
                limb(m, 0.26 * arm_len, 0.046 * b, 0.035 * b, 0.046 * b, 0.004, sleeve);
            }
            let cuff = match style {
                Style::Hero | Style::Scout | Style::Hobgoblin => Some(LEATHER),
                Style::Sentinel => Some(STEEL * 0.85),
                Style::Caster | Style::Vampire => Some(l.trim),
                _ => None,
            };
            if style == Style::Caster {
                // A bell sleeve flaring past the wrist, its edge in the trim colour.
                tube(
                    m,
                    &[Vec3::Y * -0.08, Vec3::Y * -0.17, Vec3::Y * -0.25],
                    &[(0.06 * b, 0.06 * b), (0.07 * b, 0.07 * b), (0.084 * b, 0.084 * b)],
                    sleeve,
                );
                m.torus(Vec3::Y * -0.25, Vec3::Y, 0.082 * b, 0.012, 16, 4, l.trim);
            } else if let Some(cuff) = cuff {
                m.prism(Vec3::Y * -0.13 * arm_len, Vec3::Y * -0.25 * arm_len, 0.056 * b, 0.05 * b, 8, cuff, 0.0);
                if matches!(style, Style::Hero | Style::Scout) {
                    m.prism(Vec3::Y * -0.21 * arm_len, Vec3::Y * -0.26 * arm_len, 0.052 * b, 0.064 * b, 10, cuff * 0.85, 0.0);
                }
            }
            if furry {
                p.matte.blob(Vec3::Y * -0.1, Vec3::new(0.085, 0.15, 0.085), 4, 0.1, l.cloth);
                hair_tufts(p, Vec3::Y * -0.06, 0.085, 7, Vec3::new(0.0, -1.0, 0.0), l.cloth * 0.97, 43 + side as u64);
            }
            fist(p, Vec3::Y * -0.3 * arm_len, side, b, hand_color);
            if furry {
                for k in 0..3 {
                    let x = (k as f32 - 1.0) * 0.03;
                    p.matte.prism(Vec3::new(x, -0.4 * arm_len, 0.04), Vec3::new(x, -0.45 * arm_len, 0.09), 0.012, 0.0, 4, BONE, 0.0);
                }
            }
        });
        let reach = if blocky { -0.38 } else { -0.31 * arm_len };
        let grip = kit.commands.spawn((Transform::from_translation(Vec3::Y * reach), Visibility::default())).id();
        kit.commands.entity(lower).add_child(grip);
        kit.rig.joints[hand as usize] = Some(grip);
        kit.rig.rest[hand as usize] = Vec3::Y * reach;
    }
    for (side, leg, shin) in [(1.0f32, J::LegL, J::ShinL), (-1.0, J::LegR, J::ShinR)] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, hips, Vec3::new(side * 0.095 * b * hips_w, -0.02, 0.0), &format!("leg{tag}"), |p| {
            if blocky {
                stone(&mut p.matte, Vec3::Y * -0.22, Vec3::new(0.095, 0.22, 0.095), pants, 11);
                return;
            }
            if furry {
                p.matte.blob(Vec3::ZERO, Vec3::splat(0.094 * b), 3, 0.0, pants);
                tube(
                    &mut p.matte,
                    &[Vec3::ZERO, Vec3::new(0.0, -0.13, 0.008), Vec3::Y * -0.3, Vec3::Y * -0.44],
                    &[(0.094 * b, 0.096 * b), (0.09 * b, 0.096 * b), (0.072 * b, 0.074 * b), (0.06 * b, 0.062 * b)],
                    pants,
                );
            } else {
                // Hip to knee: the thigh is fullest above the middle.
                limb(&mut p.matte, 0.44, 0.088 * b, 0.06 * b, 0.092 * b * hips_w.max(1.0), 0.008, pants);
            }
            if furry {
                p.matte.blob(Vec3::Y * -0.18, Vec3::new(0.12, 0.2, 0.12), 5, 0.1, pants);
                hair_tufts(p, Vec3::Y * -0.12, 0.11, 8, Vec3::new(0.0, -1.0, 0.0), pants * 0.97, 47 + side as u64);
            }
        });
        kit.joint(shin, thigh, Vec3::Y * -0.44, &format!("shin{tag}"), |p| {
            if blocky {
                stone(&mut p.matte, Vec3::Y * -0.2, Vec3::new(0.088, 0.2, 0.088), pants * 0.92, 12);
                stone(&mut p.matte, Vec3::new(0.0, -0.43, 0.05), Vec3::new(0.105, 0.045, 0.16), boot, 13);
                return;
            }
            if style == Style::Sentinel {
                // A knee cop with a wing.
                p.metal.blob(Vec3::new(0.0, 0.0, 0.03), Vec3::new(0.07, 0.07, 0.06), 2, 0.0, STEEL * 1.05);
                p.metal.blob(Vec3::new(side * 0.05, 0.0, 0.02), Vec3::new(0.015, 0.05, 0.04), 2, 0.0, STEEL);
            }
            // Knee, calf, boot with a turned-down cuff, foot and toe.
            if furry {
                p.matte.blob(Vec3::ZERO, Vec3::splat(0.07 * b), 4, 0.0, pants);
                tube(
                    &mut p.matte,
                    &[Vec3::ZERO, Vec3::new(0.0, -0.13, -0.012), Vec3::Y * -0.28, Vec3::Y * -0.4],
                    &[(0.062 * b, 0.064 * b), (0.07 * b, 0.076 * b), (0.054 * b, 0.056 * b), (0.05 * b, 0.05 * b)],
                    pants,
                );
            } else {
                // Knee to ankle, the calf full at the back.
                limb(&mut p.matte, 0.4, 0.062 * b, 0.048 * b, 0.068 * b, -0.014, pants);
            }
            p.satin.prism(Vec3::Y * -0.24, Vec3::Y * -0.42, 0.07 * b, 0.064 * b, 8, boot, 0.0);
            if !matches!(style, Style::Zombie | Style::Yeti) {
                p.satin.torus(Vec3::Y * -0.24, Vec3::Y, 0.072 * b, 0.016, 12, 3, boot * 1.15);
            }
            if matches!(style, Style::Hero | Style::Scout | Style::Hobgoblin) {
                for y in [-0.3f32, -0.37] {
                    p.satin.torus(Vec3::Y * y, Vec3::Y, 0.068 * b, 0.009, 12, 3, boot * 0.6);
                    p.metal.cuboid(Vec3::new(side * 0.06 * b, y, 0.02), Vec3::new(0.008, 0.014, 0.014), Quat::IDENTITY, Vec3::new(0.74, 0.5, 0.26));
                }
            }
            // A rounded boot: heel, instep and a broad toe, on a darker sole.
            p.satin.blob(Vec3::new(0.0, -0.425, 0.05 * b), Vec3::new(0.062, 0.042, 0.11) * b, 2, 0.0, boot);
            p.satin.blob(Vec3::new(0.0, -0.44, 0.13 * b), Vec3::new(0.058, 0.03, 0.055) * b, 1, 0.0, boot);
            p.satin.blob(Vec3::new(0.0, -0.468, 0.06 * b), Vec3::new(0.064, 0.008, 0.13) * b, 0, 0.0, boot * 0.55);
            if furry {
                for k in 0..3 {
                    let x = (k as f32 - 1.0) * 0.035;
                    p.matte.prism(Vec3::new(x, -0.44, 0.17), Vec3::new(x, -0.46, 0.22), 0.012, 0.0, 4, BONE, 0.0);
                }
            }
        });
    }
    // Capes and cloaks hang from the shoulders.
    if matches!(style, Style::Hero | Style::Vampire) {
        let (cloak, lining) = if style == Style::Vampire { (DARK, l.trim) } else { (l.cloth * 0.62, l.trim) };
        kit.attach(torso, Transform::from_xyz(0.0, 0.53, -0.15), "cloak", |p| {
            // The cloak hangs in soft flutes from a gathered yoke across the
            // shoulders, fanning wider toward the hem; the lining shows
            // between the folds at the edges.
            let folds = 6;
            for k in 0..folds {
                let t = k as f32 / (folds - 1) as f32 * 2.0 - 1.0;
                let top = Vec3::new(t * 0.17, 0.0, -0.01 - 0.02 * (1.0 - t * t));
                let bottom = Vec3::new(t * 0.3, -0.98 + 0.03 * (k as f32 * 1.7).sin(), -0.13 - 0.05 * (1.0 - t * t));
                let mid = top.lerp(bottom, 0.5) + Vec3::new(0.0, 0.0, -0.03);
                let c = if t.abs() > 0.9 { lining } else { cloak * (0.94 + 0.06 * (k as f32 * 2.3).sin()) };
                tube(&mut p.matte, &[top, mid, bottom], &[(0.045, 0.025), (0.055, 0.03), (0.06, 0.028)], c);
            }
            p.matte.blob(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.22, 0.04, 0.06), 2, 0.0, cloak);
        });
    }
    gear(kit, look);
}

/// What a figure carries in its hands.
fn gear(kit: &mut Kit, look: &Look) {
    let l = look.clone();
    let hand_r = kit.rig.joint(J::HandR).unwrap();
    // Held things point forward from the fist, edge up.
    let held = |grip: f32| {
        Transform::from_rotation(Quat::from_rotation_y(-FRAC_PI_2) * Quat::from_rotation_x(FRAC_PI_2))
            .with_translation(Vec3::new(0.0, -0.02, -grip))
    };
    match l.style {
        Style::Sentinel => {
            let mace = kit.library.item("mace", tor_protocol::ItemClass::Weapon, kit.meshes);
            let e = kit.commands.spawn((held(-0.28), Visibility::default())).id();
            kit.commands.entity(e).with_children(|p| mace.attach(p));
            kit.commands.entity(hand_r).add_child(e);
            let fore_l = kit.rig.joint(J::ForeL).unwrap();
            kit.attach(fore_l, Transform::from_xyz(0.06, -0.15, 0.02), "shield", |p| {
                p.matte.prism(Vec3::ZERO, Vec3::X * 0.04, 0.26, 0.26, 10, l.cloth, 0.0);
                p.metal.torus(Vec3::X * 0.04, Vec3::X, 0.25, 0.02, 14, 3, STEEL);
                p.metal.blob(Vec3::X * 0.05, Vec3::new(0.03, 0.06, 0.06), 0, 0.0, STEEL);
                emblem(&mut p.matte, Vec3::X * 0.042, Vec3::Z, Vec3::Y, Vec3::X, 0.17, Vec3::new(0.9, 0.85, 0.7), l.cloth, l.v.pattern);
            });
        }
        Style::Caster => {
            let aura = l.aura.unwrap_or(Accent::Teal);
            kit.attach(hand_r, Transform::from_xyz(0.0, 0.0, 0.0), "staff", |p| {
                p.matte.prism(Vec3::Y * -0.55, Vec3::Y * 0.85, 0.022, 0.026, 6, Vec3::new(0.3, 0.18, 0.1), 0.0);
                p.metal.torus(Vec3::Y * 0.88, Vec3::Y, 0.06, 0.012, 10, 3, Vec3::new(0.74, 0.5, 0.26));
                for k in 0..3 {
                    let a = k as f32 * TAU / 3.0;
                    p.metal.prism(Vec3::Y * 0.85, Vec3::new(a.cos() * 0.06, 1.0, a.sin() * 0.06), 0.01, 0.006, 4, Vec3::new(0.74, 0.5, 0.26), 0.0);
                }
                p.accent = Some(aura);
                p.glow.blob(Vec3::Y * 0.97, Vec3::splat(0.055), 0, 0.0, Vec3::ONE);
            });
        }
        Style::Scout => {
            let dagger = kit.library.item("dagger", tor_protocol::ItemClass::Weapon, kit.meshes);
            let e = kit.commands.spawn((held(-0.17), Visibility::default())).id();
            kit.commands.entity(e).with_children(|p| dagger.attach(p));
            kit.commands.entity(hand_r).add_child(e);
        }
        Style::Hobgoblin => {
            kit.attach(hand_r, held(0.0), "club", |p| {
                p.matte.prism(Vec3::new(-0.08, 0.0, 0.0), Vec3::new(0.45, 0.0, 0.0), 0.025, 0.06, 7, WOOD * 0.85, 0.0);
                for k in 0..4 {
                    let x = 0.25 + k as f32 * 0.05;
                    let a = k as f32 * 1.7;
                    p.metal.prism(Vec3::new(x, 0.0, 0.0), Vec3::new(x, a.cos() * 0.1, a.sin() * 0.1), 0.012, 0.0, 4, IRON, 0.0);
                }
            });
        }
        _ => {}
    }
}

/// The emu: a shaggy coat of drooping feathers on long scaly legs, an
/// S-curved blue neck and a quick beak.
fn bird(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let body = kit.joint(J::Body, lift, Vec3::new(0.0, 1.0, 0.0), "body", |p| {
        p.matte.blob(Vec3::ZERO, Vec3::new(0.25, 0.24, 0.34), 11, 0.0, l.skin * 0.7);
        // Tier upon tier of broad, drooping feathers from the crown of the
        // back to a full skirt, longest behind; each tier a shade lighter.
        for row in 0..6 {
            let lat = 0.12 + row as f32 * 0.3;
            let count = 8 + row * 3;
            for k in 0..count {
                let n = (row * 30 + k) as u32;
                let lon = (k as f32 + 0.5 * (row % 2) as f32 + rand(84, n) * 0.3) / count as f32 * TAU;
                let dir = Vec3::new(lat.sin() * lon.cos(), lat.cos(), lat.sin() * lon.sin() * 1.35).normalize();
                let root = dir * Vec3::new(0.23, 0.22, 0.32);
                let out = Vec3::new(dir.x, 0.15, dir.z).normalize_or(Vec3::NEG_Z);
                let back = (-dir.z).max(0.0);
                let len = 0.15 + 0.08 * row as f32 / 5.0 + 0.09 * back + 0.04 * rand(81, n);
                let shade = (0.78 + 0.08 * row as f32 / 5.0) + 0.3 * rand(82, n);
                feather(p, root, out, len, 0.07 + 0.012 * row as f32, l.skin * shade);
            }
        }
        for k in 0..7 {
            let a = (k as f32 - 3.0) * 0.28;
            feather(p, Vec3::new(a * 0.08, 0.08, -0.3), Vec3::new(a * 0.4, 0.0, -1.0).normalize(), 0.24, 0.07, l.skin * (0.85 + 0.05 * k as f32));
        }
    });
    let neck = kit.joint(J::Neck, body, Vec3::new(0.0, 0.12, 0.3), "neck", |p| {
        // One smooth S of a neck, thick at the base.
        tube(
            &mut p.matte,
            &[Vec3::ZERO, Vec3::new(0.0, 0.16, 0.08), Vec3::new(0.0, 0.32, 0.03), Vec3::new(0.0, 0.47, 0.08), Vec3::new(0.0, 0.5, 0.1)],
            &[(0.085, 0.09), (0.058, 0.062), (0.047, 0.05), (0.042, 0.044), (0.04, 0.04)],
            l.cloth,
        );
        // A ruff of feathers where the neck meets the body.
        for k in 0..12 {
            let a = k as f32 * TAU / 12.0;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            feather(p, out * 0.06 + Vec3::Y * 0.06, out, 0.11, 0.045, l.skin * (0.9 + 0.2 * rand(83, k)));
        }
    });
    kit.joint(J::Head, neck, Vec3::new(0.0, 0.47, 0.08), "head", |p| {
        p.matte.blob(Vec3::new(0.0, 0.03, 0.02), Vec3::new(0.06, 0.058, 0.075), 1, 0.05, l.cloth);
        p.matte.prism(Vec3::new(0.0, 0.02, 0.07), Vec3::new(0.0, 0.0, 0.17), 0.028, 0.006, 6, DARK * 2.5, 0.0);
        p.matte.prism(Vec3::new(0.0, 0.005, 0.07), Vec3::new(0.0, -0.005, 0.15), 0.018, 0.004, 5, DARK * 2.0, 0.0);
        cartoon_eyes(p, Vec3::new(0.0, 0.045, 0.035), 0.046, 0.014, Vec3::new(0.85, 0.62, 0.15));
        // A shaggy dark crest.
        hair_tufts(p, Vec3::new(0.0, 0.08, -0.01), 0.04, 7, Vec3::new(0.0, 0.6, -1.0), l.skin * 0.55, 19);
    });
    for (side, leg, shin) in [(1.0f32, J::LegL, J::ShinL), (-1.0, J::LegR, J::ShinR)] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, body, Vec3::new(side * 0.11, -0.1, 0.0), &format!("leg{tag}"), |p| {
            p.matte.blob(Vec3::new(0.0, -0.08, 0.02), Vec3::new(0.08, 0.15, 0.09), 3, 0.15, l.skin * 0.85);
            p.matte.prism(Vec3::new(0.0, -0.15, 0.03), Vec3::new(0.0, -0.4, 0.08), 0.045, 0.032, 6, l.trim, 0.0);
        });
        kit.joint(shin, thigh, Vec3::new(0.0, -0.4, 0.08), &format!("shin{tag}"), |p| {
            p.matte.blob(Vec3::ZERO, Vec3::splat(0.036), 2, 0.0, l.trim);
            p.matte.prism(Vec3::ZERO, Vec3::new(0.0, -0.46, -0.06), 0.03, 0.022, 6, l.trim * 0.95, 0.0);
            for k in 0..6 {
                let y = -0.08 - k as f32 * 0.06;
                p.matte.torus(Vec3::new(0.0, y, -0.01 * k as f32), Vec3::Y, 0.026, 0.006, 8, 3, l.trim * 0.8);
            }
            p.matte.blob(Vec3::new(0.0, -0.46, -0.06), Vec3::splat(0.03), 2, 0.0, l.trim);
            for k in -1..=1 {
                let a = k as f32 * 0.45;
                let toe = Vec3::new(a.sin() * 0.11, -0.49, -0.06 + a.cos() * 0.13);
                p.matte.prism(Vec3::new(0.0, -0.47, -0.06), toe, 0.018, 0.01, 5, l.trim * 0.8, 0.0);
                p.matte.prism(toe, toe + Vec3::new(a.sin(), -0.2, a.cos()) * 0.03, 0.01, 0.0, 4, DARK * 3.0, 0.0);
            }
        });
    }
}

/// Horses and their kin; with `centaur`, a rider's torso rises from the withers.
fn quadruped(kit: &mut Kit, lift: Entity, look: &Look, centaur: bool) {
    let l = look.clone();
    let style = l.style;
    let quagga = style == Style::Quagga;
    let h = 0.82;
    // A quagga's coat: chestnut behind, cream and brown stripes over the
    // forequarters, neck and face, a pale belly. Painted, not banded.
    let coat = move |dir: Vec3, at: Vec3| -> Vec3 {
        if !quagga {
            return l.skin;
        }
        let dark = Vec3::new(0.36, 0.2, 0.1);
        let cream = l.cloth;
        // A cream chest and belly; stripes band the sides.
        if dir.y < -0.5 || (dir.z > 0.45 && at.z > 0.2) {
            return cream;
        }
        let front = ease((at.z + 0.1) / 0.5);
        // Each quagga's stripes are its own: spacing, slant and phase.
        let pv = f32::from(l.v.pattern);
        let wave = (at.z * (30.0 + 3.0 * pv) + at.y * (4.0 + 2.5 * (pv - 1.5)) + (at.y * 5.0).sin() * 1.2 + pv * 1.7).sin();
        let striped = cream.lerp(dark, ((wave - 0.15) * 6.0).clamp(0.0, 1.0));
        l.skin.lerp(striped, front)
    };
    let body = kit.joint(J::Body, lift, Vec3::new(0.0, h + 0.12, 0.0), "body", |p| {
        // The barrel runs nose to tail: a deep chest carried low, a girth
        // that tucks up into the flank, and broad haunches.
        let path = [
            Vec3::new(0.0, -0.02, 0.6),
            Vec3::new(0.0, -0.04, 0.5),
            Vec3::new(0.0, -0.04, 0.3),
            Vec3::new(0.0, -0.01, 0.05),
            Vec3::new(0.0, 0.02, -0.25),
            Vec3::new(0.0, 0.04, -0.48),
            Vec3::new(0.0, 0.05, -0.6),
        ];
        let radii = [
            Vec2::new(0.08, 0.08),
            Vec2::new(0.17, 0.22),
            Vec2::new(0.2, 0.26),
            Vec2::new(0.2, 0.23),
            Vec2::new(0.21, 0.23),
            Vec2::new(0.18, 0.2),
            Vec2::new(0.07, 0.07),
        ];
        p.matte.tube(&path, &radii, 14, &|dir, at, _| coat(dir, at));
        let paint = |dir: Vec3, at: Vec3| coat(dir, at);
        // Breast, withers and rump.
        p.matte.blob_painted(Vec3::new(0.0, -0.09, 0.5), Vec3::new(0.15, 0.16, 0.12), 2, 0.0, &paint);
        p.matte.blob_painted(Vec3::new(0.0, 0.17, 0.32), Vec3::new(0.08, 0.07, 0.14), 3, 0.0, &paint);
        p.matte.blob_painted(Vec3::new(0.0, 0.12, -0.42), Vec3::new(0.17, 0.12, 0.2), 4, 0.0, &paint);
        for side in [-1.0f32, 1.0] {
            // The shoulder slopes down to the foreleg; the haunch rounds over the hind.
            p.matte.blob_painted(Vec3::new(side * 0.1, 0.0, 0.37), Vec3::new(0.1, 0.2, 0.15), 5, 0.0, &paint);
            p.matte.blob_painted(Vec3::new(side * 0.11, 0.03, -0.4), Vec3::new(0.12, 0.2, 0.19), 6, 0.0, &paint);
        }
    });
    let neck_at = Vec3::new(0.0, 0.12, 0.45);
    if centaur {
        let torso = kit.joint(J::Torso, body, Vec3::new(0.0, 0.15, 0.42), "torso", |p| {
            // One torso rising out of the withers: waist, ribs, chest.
            let fem = l.v.woman;
            let w = if fem { 0.88 } else { 1.0 };
            {
                // Sculpted, as the people's torsos are: the waist rising out
                // of the withers into ribs, chest and rounded shoulders.
                use crate::sdf::{Sculpt, Shape::*};
                let mut body = Sculpt::default();
                body.add(Ellipsoid(Vec3::new(0.0, -0.1, -0.02), Vec3::new(0.17, 0.16, 0.16)), 0.0)
                    .add(Ellipsoid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.15 * w, 0.14, 0.12)), 0.06)
                    .add(Ellipsoid(Vec3::new(0.0, 0.3, 0.005), Vec3::new(0.2 * w, 0.17, 0.13)), 0.06)
                    .add(Ellipsoid(Vec3::new(0.0, 0.44, -0.01), Vec3::new(0.2 * w, 0.1, 0.11)), 0.14)
                    .add(Capsule(Vec3::new(0.0, 0.5, -0.005), Vec3::new(0.0, 0.6, -0.005), 0.065, 0.055), 0.04);
                for side in [-1.0f32, 1.0] {
                    body.add(Ellipsoid(Vec3::new(side * 0.235 * w, 0.47, 0.0), Vec3::new(0.072, 0.068, 0.075)), 0.06);
                }
                if fem {
                    body.add(Ellipsoid(Vec3::new(0.0, 0.34, 0.085), Vec3::new(0.12, 0.06, 0.055)), 0.05);
                } else {
                    // Chest and stomach muscles, blended into the torso.
                    for side in [-1.0f32, 1.0] {
                        body.add(Ellipsoid(Vec3::new(side * 0.075, 0.37, 0.07), Vec3::new(0.085, 0.06, 0.05)), 0.04);
                    }
                    body.add(Ellipsoid(Vec3::new(0.0, 0.17, 0.075), Vec3::new(0.07, 0.09, 0.04)), 0.04);
                }
                body.mesh(&mut p.matte, Vec3::new(-0.34, -0.3, -0.22), Vec3::new(0.34, 0.64, 0.22), 0.0085, &|_, _| l.cloth);
            }
            if fem {
                // A wrapped leather top.
                tube(&mut p.satin, &[Vec3::Y * 0.27, Vec3::Y * 0.35, Vec3::Y * 0.43], &[(0.18, 0.15), (0.19, 0.16), (0.18, 0.14)], LEATHER);
            }
            p.matte.blob(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.2, 0.06, 0.1), 1, 0.0, l.cloth);
            p.satin.cuboid(Vec3::new(0.0, 0.3, 0.0), Vec3::new(0.24, 0.025, 0.17), Quat::from_rotation_z(0.65), LEATHER);
            p.metal.blob(Vec3::new(0.08, 0.38, 0.15), Vec3::splat(0.03), 0, 0.0, Vec3::new(0.74, 0.5, 0.26));
            p.satin.torus(Vec3::Y * 0.02, Vec3::Y, 0.18, 0.03, 12, 4, LEATHER * 0.8);
        });
        kit.joint(J::Head, torso, Vec3::Y * 0.6, "head", |p| {
            // The rider's face and hair are a person's; men wear full beards.
            let rider = Look { skin: l.cloth, ..l.clone() };
            p.matte.prism(Vec3::Y * -0.03, Vec3::Y * 0.08, 0.06, 0.052, 7, l.cloth, 0.0);
            p.set_transform(Affine3A::from_translation(Vec3::Y * 0.08) * Affine3A::from_scale(Vec3::splat(1.1)) * Affine3A::from_translation(Vec3::Y * -0.08));
            human_face(p, &rider, true);
            hair(p, &rider);
            if !rider.v.woman {
                beard(p, hair_color(&rider));
            }
        });
        for (side, arm, fore, hand) in [(1.0f32, J::ArmL, J::ForeL, J::HandL), (-1.0, J::ArmR, J::ForeR, J::HandR)] {
            let tag = if side > 0.0 { "l" } else { "r" };
            let reach = if l.v.woman { 0.215 } else { 0.24 };
            let upper = kit.joint(arm, torso, Vec3::new(side * reach, 0.48, 0.0), &format!("arm{tag}"), |p| {
                limb(&mut p.matte, 0.3, 0.064, 0.05, 0.068, 0.008, l.cloth);
            });
            let lower = kit.joint(fore, upper, Vec3::Y * -0.3, &format!("fore{tag}"), |p| {
                limb(&mut p.matte, 0.26, 0.05, 0.038, 0.056, 0.004, l.cloth);
                p.satin.prism(Vec3::Y * -0.13, Vec3::Y * -0.25, 0.058, 0.052, 10, LEATHER, 0.0);
                fist(p, Vec3::Y * -0.3, side, 1.0, l.cloth);
            });
            let grip = kit.commands.spawn((Transform::from_translation(Vec3::Y * -0.31), Visibility::default())).id();
            kit.commands.entity(lower).add_child(grip);
            kit.rig.joints[hand as usize] = Some(grip);
            // Without its rest offset the grip would be posed at the elbow.
            kit.rig.rest[hand as usize] = Vec3::Y * -0.31;
        }
        let hand = kit.rig.joint(J::HandR).unwrap();
        kit.attach(hand, Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)), "spear", |p| {
            p.matte.prism(Vec3::Y * -0.7, Vec3::Y * 0.9, 0.02, 0.02, 6, WOOD, 0.0);
            p.satin.prism(Vec3::Y * 0.78, Vec3::Y * 0.9, 0.024, 0.024, 6, LEATHER, 0.0);
            p.metal.prism(Vec3::Y * 0.9, Vec3::Y * 1.14, 0.045, 0.0, 4, STEEL, 0.0);
            p.matte.prism(Vec3::Y * 0.86, Vec3::new(0.05, 0.7, 0.0), 0.012, 0.0, 3, Vec3::new(0.6, 0.15, 0.1), 0.0);
        });
    } else {
        let neck = kit.joint(J::Neck, body, neck_at, "neck", |p| {
            let top = Vec3::new(0.0, 0.36, 0.3);
            let neck_coat = |dir: Vec3, at: Vec3| -> Vec3 {
                if quagga {
                    let wave = (at.y * 52.0 + at.z * 12.0).sin();
                    l.cloth.lerp(Vec3::new(0.38, 0.21, 0.11), ((wave - 0.05) * 3.0).clamp(0.0, 1.0))
                } else {
                    let _ = dir;
                    l.skin
                }
            };
            let path = [Vec3::new(0.0, -0.08, -0.1), Vec3::new(0.0, 0.09, 0.06), Vec3::new(0.0, 0.24, 0.19), top];
            let radii = [Vec2::new(0.14, 0.19), Vec2::new(0.115, 0.15), Vec2::new(0.09, 0.11), Vec2::new(0.072, 0.085)];
            p.matte.tube(&path, &radii, 10, &|dir, at, t| {
                if quagga && dir.z > 0.7 && t < 0.8 {
                    l.cloth
                } else {
                    let _ = t;
                    neck_coat(dir, at)
                }
            });
            let mane = if style == Style::Unicorn { l.cloth } else { l.trim };
            // The mane: locks along the crest. A unicorn's falls long and
            // sweeps to one side; a quagga's stands up in a short brush.
            let locks = if style == Style::Unicorn { 20 } else { 18 };
            for k in 0..locks {
                let t = k as f32 / (locks - 1) as f32;
                // Along the crest, the top of the arched neck.
                let root = Vec3::new(0.0, 0.06 + t * 0.36, -0.16 + t * 0.4);
                let shade = mane * (0.8 + 0.35 * rand(5, k));
                if style == Style::Unicorn {
                    let side = if k % 3 == 0 { -1.0 } else { 1.0 };
                    let len = 0.26 + 0.12 * rand(6, k);
                    let curl = 0.03 * (k as f32 * 1.3).sin();
                    // Each lock lifts off the crest, then falls close along the neck.
                    let path = [
                        root,
                        root + Vec3::new(side * 0.03, 0.05, -0.05),
                        root + Vec3::new(side * 0.08, 0.0, -0.09),
                        root + Vec3::new(side * 0.1 + curl, -len * 0.55, -0.1 + 0.05 * rand(7, k)),
                        root + Vec3::new(side * 0.09 - curl + 0.02 * rand(8, k), -len, -0.06),
                    ];
                    let r = 0.03 + 0.012 * rand(9, k);
                    tube(&mut p.matte, &path, &[(r, r * 0.7), (r * 1.1, r * 0.8), (r, r * 0.7), (r * 0.6, r * 0.5), (0.003, 0.003)], shade);
                } else {
                    let path = [root, root + Vec3::new(0.0, 0.06, -0.035), root + Vec3::new(0.0, 0.1, -0.06)];
                    tube(&mut p.matte, &path, &[(0.016, 0.03), (0.014, 0.026), (0.002, 0.004)], shade);
                }
            }
        });
        kit.joint(J::Snout, neck, Vec3::new(0.0, 0.4, 0.31), "head", |p| {
            let face = |_: Vec3, at: Vec3| -> Vec3 {
                if quagga && (at.z * 30.0 - at.y * 20.0).sin() > 0.35 && at.z < 0.2 {
                    Vec3::new(0.38, 0.21, 0.11)
                } else if quagga {
                    l.cloth
                } else {
                    l.skin
                }
            };
            // A touch larger than life, as the renders draw it.
            p.set_transform(Affine3A::from_scale(Vec3::splat(1.15)));
            p.matte.blob_painted(Vec3::new(0.0, 0.01, 0.03), Vec3::new(0.085, 0.1, 0.12), 3, 0.0, &face);
            p.matte.blob_painted(Vec3::new(0.0, -0.05, 0.03), Vec3::new(0.08, 0.08, 0.09), 2, 0.0, &face);
            p.matte.tube(
                &[Vec3::new(0.0, 0.0, 0.1), Vec3::new(0.0, -0.035, 0.22), Vec3::new(0.0, -0.07, 0.31)],
                &[Vec2::new(0.072, 0.084), Vec2::new(0.06, 0.068), Vec2::new(0.055, 0.058)],
                10,
                &|d, a, _| face(d, a),
            );
            let muzzle = if quagga { DARK * 3.0 } else { l.skin * 1.4 };
            p.matte.blob(Vec3::new(0.0, -0.08, 0.33), Vec3::new(0.062, 0.058, 0.05), 2, 0.0, muzzle);
            p.matte.blob(Vec3::new(0.0, -0.125, 0.29), Vec3::new(0.04, 0.025, 0.045), 3, 0.0, muzzle * 0.9);
            for side in [-1.0f32, 1.0] {
                p.matte.blob(Vec3::new(side * 0.028, -0.07, 0.375), Vec3::new(0.012, 0.016, 0.01), 0, 0.0, DARK);
                // Leaf-shaped ears, cupped forward, darker inside.
                let base = Vec3::new(side * 0.045, 0.08, -0.01);
                let ear = [base, base + Vec3::new(side * 0.01, 0.07, 0.0), base + Vec3::new(side * 0.02, 0.14, -0.03)];
                tube(&mut p.matte, &ear, &[(0.034, 0.016), (0.03, 0.014), (0.004, 0.004)], l.skin);
                tube(&mut p.matte, &ear.map(|v| v + Vec3::Z * 0.008), &[(0.022, 0.007), (0.019, 0.007), (0.002, 0.002)], l.skin * 0.55);
            }
            // The forelock spills over the brow.
            let lock = if style == Style::Unicorn { l.cloth } else { l.trim };
            hair_tufts(p, Vec3::new(0.0, 0.1, 0.0), 0.05, 5, Vec3::new(0.0, -0.3, 1.0), lock, 11);
            eyes(p, l.eyes, Vec3::new(0.0, 0.03, 0.09), 0.08, 0.017);
            if style == Style::Unicorn {
                p.accent = Some(Accent::Violet);
                let (a, b) = (Vec3::new(0.0, 0.09, 0.07), Vec3::new(0.0, 0.4, 0.28));
                p.metal.prism(a, b, 0.034, 0.0, 10, Vec3::new(0.12, 0.1, 0.16), 0.0);
                for k in 0..4 {
                    let t = 0.12 + k as f32 * 0.2;
                    p.glow.torus(a.lerp(b, t), (b - a).normalize(), 0.034 * (1.0 - t) + 0.005, 0.005, 12, 3, Vec3::ONE);
                }
                hair_tufts(p, Vec3::new(0.0, -0.12, 0.27), 0.025, 4, Vec3::new(0.0, -1.0, -0.2), l.cloth, 61);
            }
        });
    }
    kit.joint(J::Tail, body, Vec3::new(0.0, 0.1, -0.54), "tail", |p| {
        let c = if style == Style::Unicorn { l.cloth } else { l.trim };
        tube(&mut p.matte, &[Vec3::ZERO, Vec3::new(0.0, -0.06, -0.06), Vec3::new(0.0, -0.14, -0.09)], &[(0.05, 0.05), (0.04, 0.042), (0.03, 0.03)], l.skin);
        // A flowing switch of hair: strands that fan and curl.
        let strands = if style == Style::Unicorn { 12 } else { 9 };
        for k in 0..strands {
            let a = (k as f32 - (strands - 1) as f32 * 0.5) * 0.11;
            let sway = (k as f32 * 1.7).sin() * 0.03;
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.1, -0.08),
                    Vec3::new(a * 0.12, -0.26, -0.15),
                    Vec3::new(a * 0.25 + sway, -0.42, -0.16),
                    Vec3::new(a * 0.32 + sway, -0.56, -0.12),
                ],
                &[(0.034, 0.034), (0.034, 0.03), (0.024, 0.02), (0.002, 0.002)],
                c * (0.8 + 0.4 * rand(13, k)),
            );
        }
    });
    let legs = [
        (J::FL, J::FLs, Vec3::new(0.12, -0.1, 0.38), "fl", true),
        (J::FR, J::FRs, Vec3::new(-0.12, -0.1, 0.38), "fr", true),
        (J::BL, J::BLs, Vec3::new(0.125, -0.08, -0.4), "bl", false),
        (J::BR, J::BRs, Vec3::new(-0.125, -0.08, -0.4), "br", false),
    ];
    for (thigh, shin, at, tag, front) in legs {
        let leg_color = if quagga { l.cloth } else { l.skin };
        let upper = kit.joint(thigh, body, at, &format!("leg{tag}"), |p| {
            // A muscled upper leg tapering to a bony knee.
            let (top, mid) = if front { (0.125, 0.092) } else { (0.15, 0.11) };
            let path = if front {
                [Vec3::new(0.0, 0.16, 0.0), Vec3::new(0.0, -0.1, 0.015), Vec3::new(0.0, -0.3, 0.0), Vec3::Y * -0.42]
            } else {
                // The gaskin sweeps back and down to the hock.
                [Vec3::new(0.0, 0.1, 0.02), Vec3::new(0.0, -0.1, -0.06), Vec3::new(0.0, -0.3, -0.06), Vec3::Y * -0.42]
            };
            tube(&mut p.matte, &path, &[(top * 0.6, top * 0.8), (mid * 0.82, mid), (0.056, 0.064), (0.05, 0.058)], l.skin);
            let joint = if front { Vec3::new(0.0, -0.42, 0.012) } else { Vec3::new(0.0, -0.42, -0.03) };
            p.matte.blob(joint, Vec3::new(0.044, 0.052, 0.05), 1, 0.0, l.skin);
        });
        kit.joint(shin, upper, Vec3::Y * -0.42, &format!("shin{tag}"), |p| {
            // A slim cannon bone, the fetlock, a pastern and a flared hoof.
            tube(
                &mut p.matte,
                &[Vec3::ZERO, Vec3::Y * -0.2, Vec3::Y * -0.34, Vec3::new(0.0, -0.39, 0.012), Vec3::new(0.0, -0.42, 0.02)],
                &[(0.05, 0.056), (0.042, 0.048), (0.046, 0.054), (0.046, 0.052), (0.05, 0.054)],
                leg_color,
            );
            // A flared hoof with a darker sole.
            tube(
                &mut p.matte,
                &[Vec3::new(0.0, -0.405, 0.018), Vec3::new(0.0, -0.45, 0.026), Vec3::new(0.0, -0.48, 0.034)],
                &[(0.047, 0.051), (0.056, 0.062), (0.064, 0.072)],
                DARK * 2.4,
            );
            p.matte.blob(Vec3::new(0.0, -0.482, 0.034), Vec3::new(0.062, 0.006, 0.07), 0, 0.0, DARK * 1.4);
        });
    }
}

/// A banded snake: a smooth tapering body laid along a travelling wave, a
/// wedge-shaped head with a jaw, and a tongue that flickers.
fn serpent(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    for i in 0..SNAKE {
        let f = i as f32 / SNAKE as f32;
        let r = if i == 0 { 0.08 } else { 0.084 * (1.0 - f * 0.8) + 0.012 };
        let model = kit.part(&format!("seg{i}"), |p| {
            if i == 0 {
                // A big round head, wider than the neck, as a cartoon snake's is.
                p.set_transform(Affine3A::from_scale(Vec3::splat(1.25)));
                p.matte.blob(Vec3::new(0.0, 0.0, 0.03), Vec3::new(0.08, 0.05, 0.11), 3, 0.04, l.skin);
                p.matte.blob(Vec3::new(0.0, -0.015, 0.1), Vec3::new(0.055, 0.035, 0.07), 2, 0.0, l.skin);
                p.matte.blob(Vec3::new(0.0, -0.035, 0.05), Vec3::new(0.06, 0.02, 0.09), 1, 0.0, l.trim);
                for side in [-1.0f32, 1.0] {
                    p.matte.blob(Vec3::new(side * 0.05, 0.025, 0.06), Vec3::new(0.02, 0.012, 0.03), 0, 0.0, l.skin * 0.8);
                    p.matte.blob(Vec3::new(side * 0.02, 0.0, 0.165), Vec3::splat(0.006), 0, 0.0, DARK);
                }
                p.matte.cuboid(Vec3::new(0.0, 0.03, 0.04), Vec3::new(0.015, 0.004, 0.06), Quat::IDENTITY, l.cloth);
                for side in [-1.0f32, 1.0] {
                    // Brow ridges the eyes sit in.
                    p.matte.blob(Vec3::new(side * 0.045, 0.03, 0.065), Vec3::new(0.026, 0.022, 0.028), 2, 0.0, l.skin);
                }
                cartoon_eyes(p, Vec3::new(0.0, 0.042, 0.075), 0.047, 0.02, Vec3::new(0.9, 0.72, 0.15));
            } else {
                // Each scale band overlaps the next, with a diamond on the back.
                let band = (i / 2) % 2 == 1;
                let c = if band { l.cloth } else { l.skin };
                p.matte.blob(Vec3::ZERO, Vec3::new(r, r * 0.75, r * 1.9), i as u64, 0.02, c);
                p.matte.blob(Vec3::new(0.0, -r * 0.4, 0.0), Vec3::new(r * 0.85, r * 0.42, r * 1.8), 0, 0.0, l.trim);
                if i % 2 == 0 {
                    // A raised square scute down the spine.
                    p.matte.cuboid(Vec3::new(0.0, r * 0.74, 0.0), Vec3::new(r * 0.32, 0.01, r * 0.32), Quat::IDENTITY, l.skin * 0.62);
                }
            }
        });
        let e = kit.commands.spawn((Transform::default(), Visibility::default())).id();
        kit.commands.entity(e).with_children(|p| model.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
    // The tongue rides on the head and flicks out now and then.
    let head = kit.rig.segments[0];
    let tongue = kit.part("tongue", |p| {
        let red = Vec3::new(0.7, 0.12, 0.12);
        p.matte.prism(Vec3::ZERO, Vec3::new(0.0, 0.0, 0.08), 0.006, 0.005, 4, red, 0.0);
        for side in [-1.0f32, 1.0] {
            p.matte.prism(Vec3::new(0.0, 0.0, 0.08), Vec3::new(side * 0.015, 0.0, 0.11), 0.004, 0.0, 3, red, 0.0);
        }
    });
    let e = kit.commands.spawn((Transform::from_xyz(0.0, -0.02, 0.15), Visibility::default())).id();
    kit.commands.entity(e).with_children(|p| tongue.attach(p));
    kit.commands.entity(head).add_child(e);
    kit.rig.joints[J::Snout as usize] = Some(e);
    kit.rig.rest[J::Snout as usize] = Vec3::new(0.0, -0.02, 0.15);
}

/// Segments in a snake's body.
const SNAKE: usize = 20;

/// Things that float: a burning wisp, a drifting knot of ice.
fn floater(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let aura = l.aura.unwrap_or(Accent::Ember);
    let core = kit.joint(J::Core, lift, Vec3::Y * 1.1, "core", |p| {
        p.accent = Some(aura);
        if l.style == Style::Ice {
            for k in 0..7 {
                let a = k as f32 * TAU / 7.0;
                let dir = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
                p.glow.prism(dir * 0.05, dir * (0.3 + rand(k as u64, 3) * 0.15), 0.07, 0.0, 5, Vec3::ONE, 0.0);
            }
            p.matte.blob(Vec3::ZERO, Vec3::splat(0.16), 4, 0.2, Vec3::new(0.2, 0.3, 0.4));
            eyes(p, Some(Accent::White), Vec3::new(0.0, 0.04, 0.15), 0.06, 0.02);
            p.accent = Some(aura);
        } else {
            // Petals of flame cupping the heart, and taller tongues that curl.
            for k in 0..6 {
                let a = k as f32 * TAU / 6.0 + 0.3;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                tube(
                    &mut p.glow,
                    &[out * 0.05 - Vec3::Y * 0.09, out * 0.19 + Vec3::Y * 0.02, out * 0.12 + Vec3::Y * 0.17],
                    &[(0.05, 0.02), (0.06, 0.025), (0.004, 0.004)],
                    Vec3::ONE,
                );
            }
            for (k, h) in [(0, 0.52f32), (1, 0.36), (2, 0.4)] {
                let a = k as f32 * 2.2;
                let out = Vec3::new(a.cos(), 0.0, a.sin()) * 0.04;
                let curl = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.06;
                tube(
                    &mut p.glow,
                    &[out, out * 1.4 + Vec3::Y * h * 0.4 + curl * 0.5, out + Vec3::Y * h * 0.75 - curl * 0.3, Vec3::Y * h + curl],
                    &[(0.075, 0.05), (0.06, 0.04), (0.03, 0.02), (0.003, 0.003)],
                    Vec3::ONE,
                );
            }
        }
    });
    if l.style == Style::Wisp {
        // A brighter golden heart inside the flames.
        kit.attach(core, Transform::IDENTITY, "heart", |p| {
            p.accent = Some(Accent::Amber);
            p.glow.blob(Vec3::Y * 0.02, Vec3::splat(0.085), 1, 0.0, Vec3::ONE);
        });
    }
    if l.style == Style::Ice {
        for (side, arm) in [(1.0f32, J::ArmL), (-1.0, J::ArmR)] {
            kit.joint(arm, core, Vec3::new(side * 0.42, -0.1, 0.0), &format!("shard{side}"), |p| {
                p.accent = Some(aura);
                for k in 0..4 {
                    let dir = Vec3::new(side * 0.3, -1.0, 0.2 * (k as f32 - 1.5)).normalize();
                    p.glow.prism(Vec3::ZERO, dir * (0.35 + k as f32 * 0.04), 0.06, 0.0, 5, Vec3::ONE, 0.0);
                }
            });
        }
    }
    let mote = kit.part("mote", |p| {
        p.accent = Some(aura);
        p.glow.blob(Vec3::ZERO, Vec3::splat(0.035), 0, 0.0, Vec3::ONE);
    });
    for _ in 0..6 {
        let e = kit.commands.spawn((Transform::default(), Visibility::default())).id();
        kit.commands.entity(e).with_children(|p| mote.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
}

// ---------------------------------------------------------------------------
// Posing.

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rise from 0 to 1 over [a, b], hold, then fall back over [c, d].
fn envelope(t: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    if t < a || t > d {
        0.0
    } else if t < b {
        ease((t - a) / (b - a))
    } else if t < c {
        1.0
    } else {
        1.0 - ease((t - c) / (d - c))
    }
}

fn fwd(a: f32) -> Quat {
    Quat::from_rotation_x(-a)
}

/// Tip a joint that points up or ahead (a torso, a head, a neck, a beast's
/// body) forward and down by `a`; negative rears it back. `fwd` is the same
/// motion for limbs, which hang down from their joints.
fn bow(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

/// Rotations and offsets for one frame, keyed by joint.
struct Pose {
    rot: [Quat; JOINTS],
    off: [Vec3; JOINTS],
}

impl Pose {
    fn new() -> Self {
        Self {
            rot: [Quat::IDENTITY; JOINTS],
            off: [Vec3::ZERO; JOINTS],
        }
    }
    fn r(&mut self, j: J, q: Quat) {
        self.rot[j as usize] = self.rot[j as usize] * q;
    }
    fn o(&mut self, j: J, v: Vec3) {
        self.off[j as usize] += v;
    }
}

/// Advance timers and pose every creature.
pub fn animate(
    time: Res<Time>,
    bored: Option<Res<Boredom>>,
    mut rigs: Query<(&Rig, &mut Anim, &GlobalTransform)>,
    mut joints: Query<&mut Transform, Without<Rig>>,
) {
    let dt = time.delta_secs().min(0.05);
    // Left alone for a minute, everyone starts to amuse themselves.
    let fun = bored.map_or(0.0, |b| ease((b.0 - BORED_AFTER) / 3.0));
    for (rig, mut anim, global) in &mut rigs {
        let at = global.translation();
        let first = anim.time == 0.0;
        let moved = (at - anim.last).with_y(0.0);
        anim.last = at;
        // The dead lie still, and a body just placed hasn't moved at all.
        // A jump of more than half a cell in one frame is a placement, not a walk.
        let step = if first || anim.dead.is_some() || moved.length() > 0.5 { 0.0 } else { moved.length() };
        let target_speed = (step / dt.max(1e-4) / (4.0 * rig.scale.max(0.2))).clamp(0.0, 1.0);
        anim.speed += (target_speed - anim.speed) * (1.0 - (-dt * 10.0).exp());
        anim.walk += step * TAU * 0.9 * rig.stride / rig.scale.max(0.2);
        if step > 0.002 && anim.action.is_none() {
            anim.target_yaw = Some(moved.x.atan2(moved.z));
        }
        if let Some(goal) = anim.target_yaw {
            let mut d = (goal - anim.yaw) % TAU;
            if d > PI {
                d -= TAU;
            } else if d < -PI {
                d += TAU;
            }
            anim.yaw += d * (1.0 - (-dt * 12.0).exp());
        }
        anim.time += dt;
        if let Some((m, t)) = anim.action {
            let t = t + dt / m.duration();
            anim.action = (t < 1.0).then_some((m, t));
        }
        anim.hurt = anim.hurt.map(|t| t + dt).filter(|t| *t < 0.4);
        anim.dodge = anim.dodge.map(|t| t + dt).filter(|t| *t < 0.45);
        if let Some(d) = anim.dead.as_mut() {
            *d = (*d + dt / 1.1).min(1.0);
        }
        let pose = pose(rig, &anim, fun);
        for j in 0..JOINTS {
            if let Some(e) = rig.joints[j] {
                if let Ok(mut transform) = joints.get_mut(e) {
                    if j == J::Lift as usize {
                        // Lunges and falls are in the creature's own frame.
                        let facing = Quat::from_rotation_y(anim.yaw);
                        transform.translation = facing * pose.off[j] * rig.scale;
                        transform.rotation = facing * pose.rot[j];
                    } else {
                        transform.translation = rig.rest[j] + pose.off[j];
                        transform.rotation = pose.rot[j];
                    }
                }
            }
        }
        place_segments(rig, &anim, fun, &mut joints);
    }
}

fn pose(rig: &Rig, a: &Anim, fun: f32) -> Pose {
    let mut p = Pose::new();
    let s = a.speed;
    let w = a.walk;
    let breath = (a.time * 1.8 + a.seed).sin();
    let hurt = a.hurt.filter(|t| *t >= 0.0).map_or(0.0, |t| (1.0 - t / 0.4) * (t / 0.06).min(1.0));
    let dodge = a.dodge.filter(|t| *t >= 0.0).map_or(0.0, |t| (t / 0.45 * PI).sin());
    let (m, t) = a.action.map_or((None, 0.0), |(m, t)| (Some(m), t));
    // At rest each creature idles in its own way, fading out as it moves or acts.
    let calm = (1.0 - s).clamp(0.0, 1.0) * f32::from(u8::from(m.is_none() && a.dead.is_none()));
    if calm > 0.0 && !a.quiet {
        idle(&mut p, rig, a.time + a.seed * 3.7, a.seed, calm);
        if fun > 0.0 {
            let (w, gag, progress) = antic(a);
            if w > 0.0 {
                antics(&mut p, rig, a.time, gag, progress, w * calm * fun);
            }
        }
    }
    match rig.plan {
        Plan::Biped => biped_pose(&mut p, rig, s, w, breath, m, t),
        Plan::Bird => bird_pose(&mut p, s, w, breath, m, t),
        Plan::Quadruped | Plan::Centaur => quad_pose(&mut p, rig, s, w, breath, m, t),
        Plan::Serpent | Plan::Floater => {}
    }
    // Reactions, shared by every plan.
    p.r(J::Lift, Quat::from_rotation_x(-0.28 * hurt));
    p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * hurt));
    p.r(J::Head, bow(-0.35 * hurt));
    p.r(J::Neck, bow(-0.3 * hurt));
    let side = if a.seed > 5.0 { 1.0 } else { -1.0 };
    p.o(J::Lift, Vec3::new(side * 0.28 * dodge, 0.0, -0.05 * dodge));
    p.r(J::Lift, Quat::from_rotation_z(-side * 0.25 * dodge));
    if let Some(d) = a.dead {
        death_pose(&mut p, rig.plan, ease(d), a.seed);
    }
    p
}

/// Seconds without the player's touch before the cast starts to play.
pub const BORED_AFTER: f32 = 60.0;

/// Seconds since the player last touched keyboard or mouse.
#[derive(Resource, Default)]
pub struct Boredom(pub f32);

/// Where a creature is in its round of antics: how far into one (a weight
/// that ramps in and out), which gag, and its progress from 0 to 1.
fn antic(a: &Anim) -> (f32, u32, f32) {
    const PERIOD: f32 = 12.0;
    const START: f32 = 1.0;
    const END: f32 = 8.0;
    let local = a.time + a.seed * 3.7;
    let t = local.rem_euclid(PERIOD);
    let round = (local / PERIOD).floor() as i64;
    let gag = (round + a.seed as i64).rem_euclid(3) as u32;
    let w = ease((t - START) / 0.7) * ease((END - t) / 0.7);
    (w, gag, ((t - START) / (END - START)).clamp(0.0, 1.0))
}

/// A whole number of turns over a gag's progress, so spins start and end square.
fn spins(progress: f32, turns: f32) -> f32 {
    ease(progress) * TAU * turns
}

/// Little bits of fun while the player is away. `k` fades the gag in and
/// out; `progress` runs 0 to 1 over it, for spins and laps that must close.
fn antics(p: &mut Pose, rig: &Rig, time: f32, gag: u32, progress: f32, k: f32) {
    let beat = time * TAU * 1.6;
    match rig.plan {
        Plan::Biped => match (rig.style, gag % 2) {
            (Style::Caster, 0) => {
                // Twirls the staff like a baton.
                p.r(J::ArmR, fwd(0.9 * k) * Quat::from_rotation_z(-0.3 * k));
                p.r(J::ForeR, fwd(0.8 * k));
                p.r(J::HandR, Quat::from_rotation_z(spins(progress, 6.0) * k.signum()));
                p.r(J::Head, Quat::from_rotation_y(-0.3 * k) * bow(0.2 * k));
            }
            (Style::Caster, _) => {
                // Floats cross-legged in meditation.
                p.o(J::Lift, Vec3::Y * (0.28 + (time * 1.7).sin() * 0.04) * k);
                for (leg, shin, side) in [(J::LegL, J::ShinL, 1.0f32), (J::LegR, J::ShinR, -1.0)] {
                    p.r(leg, Quat::from_rotation_y(side * 0.7 * k) * fwd(1.5 * k));
                    p.r(shin, fwd(-2.3 * k));
                }
                for (arm, fore, side) in [(J::ArmL, J::ForeL, 1.0f32), (J::ArmR, J::ForeR, -1.0)] {
                    p.r(arm, Quat::from_rotation_z(side * 0.35 * k) * fwd(0.4 * k));
                    p.r(fore, fwd(1.0 * k));
                }
                p.r(J::Head, bow(0.15 * k));
            }
            (Style::Sentinel, 0) => {
                // Marches on the spot and salutes.
                let step = beat.sin();
                p.r(J::LegL, fwd(1.1 * step.max(0.0) * k));
                p.r(J::ShinL, fwd(-1.3 * step.max(0.0) * k));
                p.r(J::LegR, fwd(1.1 * (-step).max(0.0) * k));
                p.r(J::ShinR, fwd(-1.3 * (-step).max(0.0) * k));
                p.o(J::Lift, Vec3::Y * 0.03 * step.abs() * k);
                p.r(J::ArmR, Quat::from_rotation_z(-1.3 * k) * fwd(0.5 * k));
                p.r(J::ForeR, fwd(2.0 * k));
                p.r(J::Head, bow(-0.1 * k));
            }
            (Style::Sentinel, _) => {
                // Polishes the shield with a gauntlet.
                p.r(J::ArmL, fwd(0.9 * k) * Quat::from_rotation_z(-0.3 * k));
                p.r(J::ForeL, fwd(0.9 * k));
                let rub = time * 9.0;
                p.r(J::ArmR, fwd((0.9 + 0.2 * rub.cos()) * k) * Quat::from_rotation_z((0.25 + 0.25 * rub.sin()) * k));
                p.r(J::ForeR, fwd(1.1 * k));
                p.r(J::Head, bow(0.35 * k) * Quat::from_rotation_y(0.3 * k));
            }
            (Style::Scout, 0) => {
                // Juggles the dagger.
                let toss = (time * 9.0).sin();
                p.r(J::ArmR, fwd(0.7 * k));
                p.r(J::ForeR, fwd((1.0 + 0.6 * toss) * k));
                p.r(J::HandR, Quat::from_rotation_x(spins(progress, 9.0) * k.signum()));
                p.r(J::ArmL, fwd(0.6 * k));
                p.r(J::ForeL, fwd((1.0 - 0.6 * toss) * k));
                p.r(J::Head, bow(-0.35 * k + 0.15 * toss * k));
            }
            (Style::Scout, _) => {
                // Tiptoes in place, looking about with exaggerated stealth.
                let step = (time * 2.6).sin();
                p.o(J::Lift, Vec3::Y * -0.08 * k);
                p.r(J::LegL, fwd(0.9 * step.max(0.0) * k));
                p.r(J::ShinL, fwd(-1.4 * step.max(0.0) * k));
                p.r(J::LegR, fwd(0.9 * (-step).max(0.0) * k));
                p.r(J::ShinR, fwd(-1.4 * (-step).max(0.0) * k));
                p.r(J::Torso, bow(0.35 * k));
                for (arm, fore) in [(J::ArmL, J::ForeL), (J::ArmR, J::ForeR)] {
                    p.r(arm, fwd(0.7 * k));
                    p.r(fore, fwd(1.3 * k));
                }
                p.r(J::Head, Quat::from_rotation_y((time * 1.3).sin() * 0.8 * k));
            }
            (Style::Guardian, _) => {
                // The robot.
                let snap = |x: f32| x.sin().round();
                p.r(J::ArmL, fwd((0.8 + 0.7 * snap(time * 2.0)) * k));
                p.r(J::ForeL, fwd(1.57 * (0.5 + 0.5 * snap(time * 2.0 + 1.0)) * k));
                p.r(J::ArmR, fwd((0.8 - 0.7 * snap(time * 2.0)) * k));
                p.r(J::ForeR, fwd(1.57 * (0.5 - 0.5 * snap(time * 2.0 + 1.0)) * k));
                p.r(J::Head, Quat::from_rotation_y(0.8 * snap(time * 1.5) * k));
                p.r(J::Lift, Quat::from_rotation_y(0.35 * snap(time) * k));
            }
            (Style::Hobgoblin, 1) => {
                // Scratches its head with the club.
                p.r(J::ArmR, Quat::from_rotation_z(-1.7 * k) * fwd(0.5 * k));
                p.r(J::ForeR, fwd((2.1 + (time * 14.0).sin() * 0.15) * k));
                p.r(J::Head, Quat::from_rotation_z(0.3 * k));
                p.r(J::LegL, fwd(0.3 * k) * Quat::from_rotation_z(0.25 * k));
            }
            (Style::Zombie, 0) => {
                // Its head drops off; it catches it and puts it back.
                let drop = ease(progress * 3.0) * ease((1.0 - progress) * 3.0) * k;
                p.o(J::Head, Vec3::new(0.0, -0.62, 0.36) * drop);
                p.r(J::Head, Quat::from_rotation_x(1.2 * drop) * Quat::from_rotation_z(0.6 * drop));
                for arm in [J::ArmL, J::ArmR] {
                    p.r(arm, fwd(1.0 * k));
                }
                p.r(J::ForeL, fwd(0.5 * k));
                p.r(J::ForeR, fwd(0.5 * k));
            }
            (Style::Zombie, _) => {
                // A shuffling graveyard dance, claws up.
                let side = (time * 2.2).sin();
                p.o(J::Lift, Vec3::X * 0.12 * side * k);
                p.r(J::Lift, Quat::from_rotation_z(-0.12 * side * k));
                p.r(J::ArmL, Quat::from_rotation_z(1.0 * k) * fwd(0.6 * k));
                p.r(J::ArmR, Quat::from_rotation_z(-0.4 * k) * fwd(1.2 * k));
                p.r(J::ForeL, fwd(1.2 * k));
                p.r(J::ForeR, fwd(0.9 * k));
                p.r(J::Head, Quat::from_rotation_z(0.35 * side * k));
            }
            (Style::Vampire, 0) => {
                // A swirling pirouette, cloak flared.
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 3.0)));
                p.o(J::Lift, Vec3::Y * 0.06 * k);
                p.r(J::ArmL, Quat::from_rotation_z(1.3 * k));
                p.r(J::ArmR, Quat::from_rotation_z(-1.3 * k));
            }
            (Style::Vampire, _) => {
                // Hides behind the cloak and rises, very dramatically.
                p.o(J::Lift, Vec3::Y * 0.18 * k);
                p.r(J::ArmL, fwd(1.4 * k) * Quat::from_rotation_z(-0.7 * k));
                p.r(J::ForeL, fwd(1.5 * k));
                p.r(J::Head, bow(0.25 * k) * Quat::from_rotation_y(-0.3 * k));
            }
            (_, 0) => jig(p, time, k),
            _ => stretch(p, time, k),
        },
        Plan::Bird => match gag {
            0 => zoomies(p, progress, 0.55, k),
            _ => {
                // Head-bob dance with side steps.
                let b = beat.sin();
                p.r(J::Neck, bow((0.5 + 0.5 * b) * k));
                p.o(J::Lift, Vec3::new(0.08 * (beat * 0.5).sin(), 0.04 * b.abs(), 0.0) * k);
                p.r(J::LegL, fwd(0.4 * b.max(0.0) * k));
                p.r(J::LegR, fwd(0.4 * (-b).max(0.0) * k));
            }
        },
        Plan::Quadruped => match (rig.style, gag) {
            (_, 1) => {
                // Bucks, kicking out behind.
                let kick = (time * 5.0).sin().max(0.0) * k;
                p.r(J::Lift, Quat::from_rotation_x(0.3 * kick));
                p.o(J::Lift, Vec3::Y * 0.05 * kick);
                p.r(J::BL, fwd(-1.0 * kick));
                p.r(J::BR, fwd(-1.1 * kick));
                p.r(J::Neck, bow(0.6 * kick));
            }
            (Style::Unicorn, _) | (_, 0) => {
                // Rears up, pawing the air.
                p.r(J::Lift, Quat::from_rotation_x(-0.65 * k));
                p.o(J::Lift, Vec3::new(0.0, 0.18, -0.3) * k);
                let paw = (time * 8.0).sin();
                p.r(J::FL, fwd((1.1 + 0.4 * paw) * k));
                p.r(J::FLs, fwd(-1.5 * k));
                p.r(J::FR, fwd((1.1 - 0.4 * paw) * k));
                p.r(J::FRs, fwd(-1.5 * k));
                p.r(J::Neck, bow(-0.4 * k));
            }
            _ => zoomies(p, progress, 0.7, k),
        },
        Plan::Centaur => {
            // A hoof-stamping dance, arms high.
            let b = beat.sin();
            p.r(J::FL, fwd(0.8 * b.max(0.0) * k));
            p.r(J::FR, fwd(0.8 * (-b).max(0.0) * k));
            p.r(J::Torso, Quat::from_rotation_y(0.4 * b * k));
            p.r(J::ArmL, Quat::from_rotation_z(2.4 * k));
            p.r(J::ArmR, Quat::from_rotation_z(-2.4 * k));
            p.r(J::Head, Quat::from_rotation_z(0.2 * b * k));
        }
        Plan::Serpent => {
            if gag != 0 {
                // Chases its own tail.
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 3.0)));
            }
        }
        Plan::Floater => match gag {
            0 => {
                // Loop the loop.
                let a = spins(progress, 2.0);
                p.o(J::Lift, Vec3::new(0.0, (1.0 - a.cos()) * 0.45, a.sin() * 0.45));
            }
            _ => {
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 5.0)));
                p.o(J::Lift, Vec3::Y * 0.4 * k);
            }
        },
    }
}

/// A jig: hops, kicks and waving arms.
fn jig(p: &mut Pose, time: f32, k: f32) {
    let b = (time * TAU * 1.6).sin();
    p.o(J::Lift, Vec3::Y * 0.07 * b.abs() * k);
    p.r(J::LegL, fwd(0.7 * b.max(0.0) * k));
    p.r(J::ShinL, fwd(-0.9 * b.max(0.0) * k));
    p.r(J::LegR, fwd(0.7 * (-b).max(0.0) * k));
    p.r(J::ShinR, fwd(-0.9 * (-b).max(0.0) * k));
    p.r(J::ArmL, Quat::from_rotation_z((1.6 + 0.5 * b) * k));
    p.r(J::ArmR, Quat::from_rotation_z(-(1.6 - 0.5 * b) * k));
    p.r(J::ForeL, fwd(0.6 * k));
    p.r(J::ForeR, fwd(0.6 * k));
    p.r(J::Torso, Quat::from_rotation_z(0.12 * b * k));
    p.r(J::Head, Quat::from_rotation_z(-0.2 * b * k));
}

/// A long yawn and a stretch, arms over the head.
fn stretch(p: &mut Pose, time: f32, k: f32) {
    let wave = (time * 1.5).sin() * 0.1;
    p.r(J::ArmL, Quat::from_rotation_z((2.7 + wave) * k));
    p.r(J::ArmR, Quat::from_rotation_z(-(2.7 - wave) * k));
    p.r(J::ForeL, Quat::from_rotation_z(0.3 * k));
    p.r(J::ForeR, Quat::from_rotation_z(-0.3 * k));
    p.r(J::Torso, bow(-0.25 * k) * Quat::from_rotation_z(wave * k));
    p.r(J::Head, bow(-0.45 * k));
    p.o(J::Lift, Vec3::Y * 0.03 * k);
}

/// Racing round in a little circle, legs a blur.
fn zoomies(p: &mut Pose, progress: f32, r: f32, k: f32) {
    let a = spins(progress, 2.0);
    p.o(J::Lift, Vec3::new((1.0 - a.cos()) * r, 0.0, a.sin() * r));
    p.r(J::Lift, Quat::from_rotation_y(a));
    let run = progress * 120.0;
    for (j, phase) in [(J::LegL, 0.0), (J::LegR, PI), (J::FL, 0.0), (J::FR, PI), (J::BL, PI), (J::BR, 0.0)] {
        p.r(j, fwd((run + phase).sin() * 0.8 * k));
    }
    p.r(J::Neck, bow(0.4 * k));
}

/// A bell over part of a repeating cycle: 0 outside [start, end] of each
/// `period` seconds, rising smoothly to 1 in the middle.
fn every(time: f32, period: f32, start: f32, end: f32) -> f32 {
    let t = time.rem_euclid(period);
    if t < start || t > end {
        0.0
    } else {
        ((t - start) / (end - start) * PI).sin().powi(2)
    }
}

/// Idle behaviour: breathing, weight shifts and looking about, with a
/// signature fidget every few seconds that fits the creature.
fn idle(p: &mut Pose, rig: &Rig, time: f32, seed: f32, k: f32) {
    let look = (time * 0.37).sin() * 0.3 + (time * 0.13).sin() * 0.2;
    let sway = (time * 0.6).sin();
    match rig.plan {
        Plan::Biped => {
            p.r(J::Lift, Quat::from_rotation_z(sway * 0.025 * k));
            p.r(J::Hips, Quat::from_rotation_z(-sway * 0.04 * k));
            p.r(J::LegL, Quat::from_rotation_z(sway * 0.03 * k));
            p.r(J::LegR, Quat::from_rotation_z(sway * 0.03 * k));
            p.r(J::Head, Quat::from_rotation_y(look * k));
            match rig.style {
                Style::Hero => {
                    // Lifts the blade to look along its edge.
                    let f = every(time, 8.0, 4.5, 7.0) * k;
                    p.r(J::ArmR, fwd(0.9 * f) * Quat::from_rotation_z(0.25 * f));
                    p.r(J::ForeR, fwd(0.9 * f));
                    p.r(J::HandR, fwd(0.9 * f) * Quat::from_rotation_z(0.6 * f));
                    p.r(J::Head, Quat::from_rotation_y(-0.35 * f) * bow(0.15 * f));
                }
                Style::Caster => {
                    // Raises a palm to the orb, which it studies.
                    let f = every(time, 7.0, 3.5, 6.3) * k;
                    p.r(J::ArmL, fwd(1.1 * f) * Quat::from_rotation_z(0.2 * f));
                    p.r(J::ForeL, fwd(0.8 * f + (time * 5.0).sin() * 0.08 * f));
                    p.r(J::ArmR, fwd(0.25 * f));
                    p.r(J::Head, Quat::from_rotation_y(0.25 * f) * bow(-0.1 * f));
                }
                Style::Sentinel => {
                    // Settles the shield and rolls a shoulder.
                    let f = every(time, 6.5, 3.8, 5.8) * k;
                    p.r(J::ArmL, fwd(0.55 * f));
                    p.r(J::ForeL, fwd(0.7 * f));
                    p.r(J::Torso, Quat::from_rotation_z(0.06 * f));
                }
                Style::Scout => {
                    // Light on its feet, glancing sharply about; flips the dagger.
                    let dart = ((time * 0.9).sin() * 3.0).clamp(-1.0, 1.0) * 0.45;
                    p.r(J::Head, Quat::from_rotation_y(dart * k));
                    p.o(J::Lift, Vec3::Y * -0.03 * k);
                    let f = every(time, 5.5, 3.0, 4.2) * k;
                    p.r(J::HandR, Quat::from_rotation_y(f * TAU));
                    p.r(J::ArmR, fwd(0.4 * f));
                }
                Style::Guardian => {
                    // A slow, grinding scan of the room.
                    p.r(J::Head, Quat::from_rotation_y((time * 0.22).sin() * 0.6 * k));
                    p.r(J::Torso, Quat::from_rotation_y((time * 0.22).sin() * 0.12 * k));
                }
                Style::Hobgoblin => {
                    // Twitchy, and taps the club on its shoulder.
                    let twitch = ((time * 4.3).sin() * (time * 1.7).sin()).max(0.0) * 0.25;
                    p.r(J::Head, Quat::from_rotation_z(twitch * k));
                    let f = every(time, 6.0, 2.5, 5.0) * k;
                    let tap = (time * 9.0).sin().max(0.0) * 0.15;
                    p.r(J::ArmR, fwd(-0.4 * f) * Quat::from_rotation_z(-0.3 * f));
                    p.r(J::ForeR, fwd((1.9 + tap) * f));
                }
                Style::Zombie => {
                    // Sways and lolls.
                    p.r(J::Lift, Quat::from_rotation_z((time * 0.8).sin() * 0.08 * k));
                    p.r(J::Head, Quat::from_rotation_z((time * 0.55).sin() * 0.35 * k) * bow(0.15 * k));
                    p.r(J::ArmL, fwd((time * 0.7).sin() * 0.15 * k));
                    p.r(J::ArmR, fwd((time * 0.7 + 1.5).sin() * 0.15 * k));
                }
                Style::Vampire => {
                    // Still and watchful, with a slow tilt of the head.
                    p.r(J::Head, Quat::from_rotation_z((time * 0.3).sin() * 0.12 * k));
                    p.o(J::Lift, Vec3::Y * ((time * 0.9).sin() * 0.012 * k));
                    let f = every(time, 9.0, 5.0, 8.0) * k;
                    p.r(J::ArmL, fwd(0.9 * f) * Quat::from_rotation_z(-0.5 * f));
                    p.r(J::ForeL, fwd(1.3 * f));
                }
                Style::Yeti => {
                    // Heavy breaths, and now and then beats its chest.
                    p.r(J::Torso, bow((time * 1.2).sin() * 0.05 * k));
                    let f = every(time, 9.0, 5.5, 7.5) * k;
                    let beat = (time * 12.0).sin();
                    p.r(J::ArmL, fwd((0.9 + 0.25 * beat) * f));
                    p.r(J::ArmR, fwd((0.9 - 0.25 * beat) * f));
                    p.r(J::ForeL, fwd(1.2 * f));
                    p.r(J::ForeR, fwd(1.2 * f));
                    p.r(J::Head, bow(-0.3 * f));
                }
                _ => {}
            }
        }
        Plan::Bird => {
            // Head bobs and jerks; every few seconds it pecks at the ground.
            let jerk = ((time * 1.3).sin() * 4.0).round() * 0.08;
            p.r(J::Neck, Quat::from_rotation_y(jerk * k));
            let f = every(time, 5.0, 2.8, 3.8) * k;
            let peck = (time * 14.0).sin().max(0.0);
            p.r(J::Neck, bow((1.6 + 0.2 * peck) * f));
            p.r(J::Head, bow(0.4 * f));
            p.r(J::Body, bow(0.25 * f));
        }
        Plan::Quadruped | Plan::Centaur => {
            p.r(J::Tail, Quat::from_rotation_y((time * 2.1).sin() * 0.35 * k));
            p.r(J::Body, Quat::from_rotation_z(sway * 0.02 * k));
            if rig.plan == Plan::Centaur {
                p.r(J::Head, Quat::from_rotation_y(look * k));
                return;
            }
            // Grazes, then lifts its head; stamps a forefoot now and then.
            let graze = every(time, 10.0, 4.0, 8.5) * k;
            let chew = (time * 6.0).sin() * 0.05 * graze;
            p.r(J::Neck, bow((1.15 + chew) * graze) * Quat::from_rotation_y(look * 0.4 * (k - graze)));
            p.r(J::Snout, bow(0.35 * graze));
            let stamp = every(time + seed, 7.0, 1.0, 1.6) * k;
            p.r(J::FL, fwd(0.5 * stamp));
            p.r(J::FLs, fwd(-0.9 * stamp));
        }
        Plan::Serpent | Plan::Floater => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn biped_pose(p: &mut Pose, rig: &Rig, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32) {
    let zombie = rig.style == Style::Zombie;
    let lumber = matches!(rig.style, Style::Guardian | Style::Yeti);
    let stride = if zombie { 0.38 } else { 0.55 };
    let swing = w.sin() * stride * s;
    p.r(J::LegL, fwd(swing));
    p.r(J::LegR, fwd(-swing * if zombie { 0.6 } else { 1.0 }));
    p.r(J::ShinL, fwd(-((w + 0.9).sin().max(0.0) * 0.9 * s)));
    p.r(J::ShinR, fwd(-((w + 0.9 + PI).sin().max(0.0) * 0.9 * s)));
    let bob = (w * 2.0).cos().abs() * 0.04 * s;
    p.o(J::Lift, Vec3::Y * (bob - 0.02 * s));
    p.r(J::Torso, Quat::from_rotation_y(w.sin() * 0.12 * s) * bow(rig.hunch + 0.08 * s + breath * 0.015));
    if lumber {
        p.r(J::Lift, Quat::from_rotation_z(w.sin() * 0.06 * s));
    }
    if zombie {
        // Arms out ahead, swaying; a dragging gait.
        p.r(J::ArmL, fwd(1.25 + breath * 0.05) * Quat::from_rotation_z(0.1));
        p.r(J::ArmR, fwd(1.15 - breath * 0.05) * Quat::from_rotation_z(-0.1));
        p.r(J::Head, Quat::from_rotation_z(0.25 + breath * 0.05));
        p.r(J::Lift, Quat::from_rotation_z(w.sin() * 0.1 * s));
    } else {
        p.r(J::ArmL, fwd(-swing * 0.7) * Quat::from_rotation_z(0.1 + breath * 0.01));
        p.r(J::ArmR, fwd(swing * 0.7) * Quat::from_rotation_z(-0.1 - breath * 0.01));
        p.r(J::ForeL, fwd(0.25 + 0.2 * s));
        p.r(J::ForeR, fwd(0.3 + 0.2 * s));
    }
    p.r(J::Head, bow(-rig.hunch * 0.7 + breath * 0.02));
    // At rest the weapon hangs point-down from the fist; a move takes it over.
    let busy = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.85, 1.0));
    if !zombie {
        p.r(J::HandR, fwd(-0.8 * (1.0 - busy)));
    }
    let Some(m) = m else {
        return;
    };
    match m {
        Move::Strike => {
            let wind = envelope(t, 0.0, 0.38, 0.38, 0.4);
            let hit = envelope(t, 0.38, 0.5, 0.6, 1.0);
            p.r(J::ArmR, fwd(-2.1 * wind + 1.25 * hit) * Quat::from_rotation_z(-0.3 * wind));
            p.r(J::ForeR, fwd(0.9 * wind + 0.1 * hit));
            p.r(J::HandR, fwd(-0.5 * wind + 1.2 * hit));
            p.r(J::Torso, Quat::from_rotation_y(0.4 * wind - 0.35 * hit) * bow(-0.1 * wind + 0.25 * hit));
            p.o(J::Lift, Vec3::new(0.0, 0.0, -0.06 * wind + 0.3 * hit));
            p.r(J::LegR, fwd(-0.3 * hit));
            p.r(J::LegL, fwd(0.45 * hit));
        }
        Move::Heavy => {
            let up = envelope(t, 0.0, 0.45, 0.45, 0.48);
            let slam = envelope(t, 0.45, 0.56, 0.7, 1.0);
            for (arm, fore, hand) in [(J::ArmL, J::ForeL, J::HandL), (J::ArmR, J::ForeR, J::HandR)] {
                p.r(arm, fwd(-2.7 * up + 1.0 * slam));
                p.r(fore, fwd(0.5 * up));
                p.r(hand, fwd(-0.4 * up + 1.1 * slam));
            }
            p.r(J::ArmL, Quat::from_rotation_z(-0.3 * (up + slam)));
            p.r(J::ArmR, Quat::from_rotation_z(0.3 * (up + slam)));
            p.r(J::Torso, bow(-0.25 * up + 0.55 * slam));
            p.o(J::Lift, Vec3::new(0.0, 0.05 * up - 0.18 * slam, 0.25 * slam));
            p.r(J::LegL, fwd(0.5 * slam));
            p.r(J::ShinL, fwd(-0.6 * slam));
            p.r(J::ShinR, fwd(-0.4 * slam));
        }
        Move::Cast(_) => {
            let gather = envelope(t, 0.0, 0.4, 0.42, 0.5);
            let release = envelope(t, 0.42, 0.5, 0.65, 1.0);
            p.r(J::ArmR, fwd(0.9 * gather + 1.5 * release) * Quat::from_rotation_z(-0.2 * gather));
            p.r(J::ArmL, fwd(1.1 * gather + 0.5 * release) * Quat::from_rotation_z(0.4 * gather));
            p.r(J::ForeL, fwd(1.0 * gather));
            p.r(J::Torso, Quat::from_rotation_y(0.25 * gather - 0.2 * release) * bow(-0.12 * gather + 0.18 * release));
            p.r(J::Head, bow(-0.15 * gather));
            p.o(J::Lift, Vec3::new(0.0, 0.04 * gather, 0.12 * release));
        }
        Move::Hex => {
            let k = envelope(t, 0.0, 0.35, 0.7, 1.0);
            p.r(J::ArmL, fwd(1.3 * k) * Quat::from_rotation_z(0.6 * k));
            p.r(J::ArmR, fwd(1.3 * k) * Quat::from_rotation_z(-0.6 * k));
            p.r(J::ForeL, fwd(-0.2 * k));
            p.r(J::ForeR, fwd(-0.2 * k));
            p.r(J::Head, bow(0.2 * k) * Quat::from_rotation_z((t * 18.0).sin() * 0.04 * k));
            p.o(J::Lift, Vec3::Y * 0.06 * k);
        }
        Move::Guard(_) => {
            let k = envelope(t, 0.0, 0.2, 0.75, 1.0);
            p.o(J::Lift, Vec3::new(0.0, -0.13 * k, -0.04 * k));
            p.r(J::ArmL, fwd(1.25 * k) * Quat::from_rotation_z(-0.55 * k));
            p.r(J::ForeL, fwd(1.0 * k));
            p.r(J::ArmR, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.6 * k));
            p.r(J::Torso, bow(0.2 * k));
            p.r(J::LegL, fwd(0.45 * k));
            p.r(J::ShinL, fwd(-0.7 * k));
            p.r(J::LegR, fwd(-0.25 * k));
            p.r(J::ShinR, fwd(-0.5 * k));
        }
        Move::Thrust => {
            let draw = envelope(t, 0.0, 0.3, 0.3, 0.34);
            let lunge = envelope(t, 0.32, 0.4, 0.55, 1.0);
            p.r(J::ArmR, fwd(-0.5 * draw + 1.45 * lunge) * Quat::from_rotation_z(-0.15 * draw));
            p.r(J::ForeR, fwd(1.3 * draw));
            p.r(J::HandR, fwd(0.2 * draw + 1.45 * lunge));
            p.r(J::ArmL, fwd(-0.6 * lunge) * Quat::from_rotation_z(0.5 * lunge));
            p.r(J::Torso, Quat::from_rotation_y(0.35 * draw - 0.45 * lunge) * bow(0.15 * lunge));
            p.o(J::Lift, Vec3::new(0.0, -0.08 * lunge, -0.1 * draw + 0.45 * lunge));
            p.r(J::LegL, fwd(0.75 * lunge));
            p.r(J::ShinL, fwd(-0.5 * lunge));
            p.r(J::LegR, fwd(-0.45 * lunge));
        }
        Move::Combo(other) => {
            // A backhand: the arm crosses the body, then sweeps out.
            let (arm, fore, hand, sign) = if other {
                (J::ArmL, J::ForeL, J::HandL, -1.0)
            } else {
                (J::ArmR, J::ForeR, J::HandR, 1.0)
            };
            let cross = envelope(t, 0.0, 0.3, 0.3, 0.33);
            let sweep = envelope(t, 0.32, 0.42, 0.55, 1.0);
            p.r(arm, fwd(0.9 * cross + 1.1 * sweep) * Quat::from_rotation_z(sign * (1.1 * cross - 0.9 * sweep)));
            p.r(fore, fwd(1.1 * cross + 0.2 * sweep));
            p.r(hand, fwd(0.6 * sweep));
            p.r(J::Torso, Quat::from_rotation_y(sign * (-0.5 * cross + 0.55 * sweep)));
            p.o(J::Lift, Vec3::new(0.0, -0.04 * sweep, 0.2 * sweep));
            p.r(J::LegL, fwd(0.3 * sweep));
        }
        Move::Hook => {
            let low = envelope(t, 0.0, 0.35, 0.4, 0.8);
            let rip = envelope(t, 0.38, 0.48, 0.55, 1.0);
            p.o(J::Lift, Vec3::new(0.0, -0.18 * low, 0.25 * rip));
            p.r(J::ArmR, fwd(0.3 * low + 0.9 * rip) * Quat::from_rotation_z(-0.9 * low + 1.1 * rip));
            p.r(J::ForeR, fwd(0.4 * low + 0.8 * rip));
            p.r(J::HandR, fwd(0.9 * low) * Quat::from_rotation_y(1.2 * rip));
            p.r(J::Torso, Quat::from_rotation_y(0.5 * low - 0.6 * rip) * bow(0.35 * low));
            p.r(J::LegL, fwd(0.6 * low));
            p.r(J::ShinL, fwd(-0.9 * low));
            p.r(J::ShinR, fwd(-0.6 * low));
        }
        Move::Bash => {
            let set = envelope(t, 0.0, 0.3, 0.3, 0.33);
            let punch = envelope(t, 0.32, 0.42, 0.55, 1.0);
            p.r(J::ArmL, fwd(0.8 * set + 1.45 * punch) * Quat::from_rotation_z(-0.45 * set - 0.2 * punch));
            p.r(J::ForeL, fwd(1.0 * set - 0.1 * punch));
            p.r(J::ArmR, fwd(-0.6 * set));
            p.r(J::Torso, Quat::from_rotation_y(-0.3 * set + 0.3 * punch) * bow(0.25 * punch));
            p.o(J::Lift, Vec3::new(0.0, -0.06 * set, -0.08 * set + 0.42 * punch));
            p.r(J::LegL, fwd(0.6 * punch));
            p.r(J::LegR, fwd(-0.4 * punch));
        }
        Move::Shove => {
            let load = envelope(t, 0.0, 0.32, 0.32, 0.35);
            let drive = envelope(t, 0.34, 0.45, 0.6, 1.0);
            for (arm, fore) in [(J::ArmL, J::ForeL), (J::ArmR, J::ForeR)] {
                p.r(arm, fwd(0.7 * load + 1.5 * drive));
                p.r(fore, fwd(1.3 * load - 0.2 * drive));
            }
            p.r(J::ArmL, Quat::from_rotation_z(-0.2 * drive));
            p.r(J::ArmR, Quat::from_rotation_z(0.2 * drive));
            p.r(J::Torso, bow(0.2 * load + 0.45 * drive));
            p.o(J::Lift, Vec3::new(0.0, -0.12 * load - 0.05 * drive, -0.12 * load + 0.5 * drive));
            p.r(J::LegL, fwd(0.5 * load + 0.8 * drive));
            p.r(J::ShinL, fwd(-0.7 * load - 0.3 * drive));
            p.r(J::LegR, fwd(-0.6 * drive));
        }
        Move::Focus => {
            let k = envelope(t, 0.0, 0.25, 0.8, 1.0);
            p.r(J::ArmR, fwd(1.2 * k) * Quat::from_rotation_z(0.35 * k));
            p.r(J::ForeR, fwd(1.0 * k));
            p.r(J::HandR, fwd(-0.7 * k));
            p.r(J::ArmL, fwd(0.5 * k) * Quat::from_rotation_z(0.3 * k));
            p.r(J::Head, Quat::from_rotation_z(0.18 * k * (t * 6.0).sin()) * bow(0.1 * k));
            p.r(J::Torso, Quat::from_rotation_y(0.25 * k));
            p.o(J::Lift, Vec3::new(0.0, -0.08 * k, 0.0));
            p.r(J::LegL, fwd(0.35 * k));
            p.r(J::LegR, fwd(-0.3 * k));
            p.r(J::ShinR, fwd(-0.3 * k));
        }
        Move::Fear => {
            let draw = envelope(t, 0.0, 0.3, 0.3, 0.35);
            let cry = envelope(t, 0.33, 0.42, 0.75, 1.0);
            p.r(J::ArmL, fwd(-0.4 * cry - 0.2 * draw) * Quat::from_rotation_z(1.3 * cry + 0.3 * draw));
            p.r(J::ArmR, fwd(-0.4 * cry - 0.2 * draw) * Quat::from_rotation_z(-1.3 * cry - 0.3 * draw));
            p.r(J::ForeL, fwd(0.6 * cry));
            p.r(J::ForeR, fwd(0.6 * cry));
            p.r(J::Torso, bow(0.35 * draw - 0.3 * cry));
            p.r(J::Head, bow(0.3 * draw + 0.35 * cry) * Quat::from_rotation_z((t * 40.0).sin() * 0.05 * cry));
            p.o(J::Lift, Vec3::new(0.0, 0.06 * cry, 0.12 * cry));
        }
        Move::Drain => {
            let reach = envelope(t, 0.0, 0.3, 0.45, 0.6);
            let draw = envelope(t, 0.4, 0.6, 0.85, 1.0);
            p.r(J::ArmR, fwd(1.5 * reach + 0.9 * draw) * Quat::from_rotation_z(-0.1 * reach));
            p.r(J::ForeR, fwd(-0.1 * reach + 1.6 * draw));
            p.r(J::ArmL, fwd(0.9 * reach) * Quat::from_rotation_z(0.4 * reach));
            p.r(J::Torso, bow(0.25 * reach - 0.2 * draw));
            p.r(J::Head, bow(0.15 * reach - 0.45 * draw));
            p.o(J::Lift, Vec3::new(0.0, 0.05 * draw, 0.25 * reach - 0.1 * draw));
        }
        Move::Touch(_) => {
            let coil = envelope(t, 0.0, 0.3, 0.3, 0.35);
            let reach = envelope(t, 0.33, 0.45, 0.6, 1.0);
            p.r(J::ArmR, fwd(0.3 * coil + 1.45 * reach));
            p.r(J::ForeR, fwd(1.2 * coil));
            p.r(J::Torso, Quat::from_rotation_y(0.3 * coil - 0.25 * reach) * bow(0.2 * reach));
            p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * coil + 0.35 * reach));
            p.r(J::LegL, fwd(0.45 * reach));
        }
        Move::Step => {
            let back = envelope(t, 0.0, 0.35, 0.7, 1.0);
            let lift = (t / 0.35 * PI).sin().max(0.0);
            p.o(J::Lift, Vec3::new(0.0, 0.05 * lift - 0.08 * back, -0.4 * back));
            p.r(J::LegR, fwd(-0.6 * lift));
            p.r(J::ShinR, fwd(-0.8 * lift));
            p.r(J::ArmL, fwd(1.1 * back) * Quat::from_rotation_z(-0.5 * back));
            p.r(J::ForeL, fwd(0.9 * back));
            p.r(J::ArmR, fwd(0.7 * back));
            p.r(J::ForeR, fwd(0.6 * back));
            p.r(J::Torso, bow(0.15 * back));
        }
        Move::Mend => {
            // Down on one knee, hands working a bandage round the arm.
            let kneel = envelope(t, 0.0, 0.25, 0.8, 1.0);
            let wrap = (t * 14.0).sin() * kneel;
            p.o(J::Lift, Vec3::new(0.0, -0.4 * kneel, 0.0));
            p.r(J::LegL, fwd(1.35 * kneel));
            p.r(J::ShinL, fwd(-1.45 * kneel));
            p.r(J::LegR, fwd(-0.15 * kneel));
            p.r(J::ShinR, fwd(-1.5 * kneel));
            p.r(J::ArmL, fwd(0.9 * kneel) * Quat::from_rotation_z(-0.35 * kneel));
            p.r(J::ForeL, fwd(1.1 * kneel));
            p.r(J::ArmR, fwd((0.9 + 0.15 * wrap) * kneel) * Quat::from_rotation_z((0.5 + 0.2 * wrap) * kneel));
            p.r(J::ForeR, fwd(1.3 * kneel));
            p.r(J::Torso, bow(0.3 * kneel));
            p.r(J::Head, bow(0.45 * kneel));
        }
    }
}

fn bird_pose(p: &mut Pose, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32) {
    let swing = w.sin() * 0.6 * s;
    p.r(J::LegL, fwd(swing));
    p.r(J::LegR, fwd(-swing));
    p.r(J::ShinL, fwd(((w + 1.0).sin().max(0.0)) * 0.9 * s));
    p.r(J::ShinR, fwd(((w + 1.0 + PI).sin().max(0.0)) * 0.9 * s));
    p.o(J::Lift, Vec3::Y * ((w * 2.0).cos().abs() * 0.05 * s));
    p.r(J::Body, Quat::from_rotation_z(w.sin() * 0.06 * s) * bow(0.1 * s + breath * 0.02));
    // The head pumps with each step and twitches when still.
    let twitch = ((breath * 3.0).sin() * 3.0).round() * 0.06 * (1.0 - s);
    p.r(J::Neck, bow((w * 2.0).sin() * 0.18 * s + 0.1 * s) * Quat::from_rotation_y(twitch));
    p.r(J::Head, bow(-(w * 2.0).sin() * 0.15 * s));
    if let Some(m) = m {
        let wind = envelope(t, 0.0, 0.35, 0.35, 0.38);
        let peck = envelope(t, 0.38, 0.47, 0.55, 1.0);
        match m {
            m if m.offensive() => {
                p.r(J::Neck, bow(-0.7 * wind + 1.5 * peck));
                p.r(J::Head, bow(0.3 * wind + 0.4 * peck));
                p.r(J::Body, bow(-0.2 * wind + 0.35 * peck));
                p.o(J::Lift, Vec3::new(0.0, 0.0, 0.3 * peck));
                if matches!(m, Move::Heavy | Move::Shove | Move::Hook) {
                    p.r(J::LegR, fwd(1.3 * peck));
                }
            }
            _ => {
                let k = envelope(t, 0.0, 0.3, 0.7, 1.0);
                p.r(J::Neck, bow(-0.5 * k));
                p.r(J::Body, bow(-0.25 * k));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn quad_pose(p: &mut Pose, rig: &Rig, s: f32, w: f32, breath: f32, m: Option<Move>, t: f32) {
    let a = w.sin() * 0.45 * s;
    let b = (w + PI).sin() * 0.45 * s;
    p.r(J::FL, fwd(a));
    p.r(J::BR, fwd(a));
    p.r(J::FR, fwd(b));
    p.r(J::BL, fwd(b));
    p.r(J::FLs, fwd(-((w + 0.8).sin().max(0.0)) * 0.8 * s));
    p.r(J::FRs, fwd(-((w + 0.8 + PI).sin().max(0.0)) * 0.8 * s));
    p.r(J::BLs, fwd(((w + 0.8 + PI).sin().max(0.0)) * 0.6 * s));
    p.r(J::BRs, fwd(((w + 0.8).sin().max(0.0)) * 0.6 * s));
    p.o(J::Lift, Vec3::Y * ((w * 2.0).cos().abs() * 0.04 * s));
    p.r(J::Body, bow(breath * 0.01));
    p.r(J::Neck, bow((w * 2.0).sin() * 0.08 * s + breath * 0.03));
    p.r(J::Snout, bow(breath * 0.04));
    p.r(J::Tail, Quat::from_rotation_y((w * 1.5).sin() * 0.3 * s + breath * 0.15));
    // A centaur's rider swings arms like a walker.
    p.r(J::ArmL, fwd(-a * 0.6) * Quat::from_rotation_z(0.1));
    p.r(J::ArmR, fwd(0.4) * Quat::from_rotation_z(-0.1));
    p.r(J::ForeR, fwd(0.8));
    let Some(m) = m else {
        return;
    };
    if !m.offensive() {
        let k = envelope(t, 0.0, 0.25, 0.75, 1.0);
        p.r(J::Body, bow(-0.15 * k));
        p.o(J::Lift, Vec3::new(0.0, -0.06 * k, -0.25 * k * f32::from(u8::from(m == Move::Step))));
        p.r(J::Neck, bow(-0.4 * k));
        p.r(J::ArmL, fwd(1.0 * k));
        return;
    }
    if rig.plan == Plan::Centaur {
        let wind = envelope(t, 0.0, 0.38, 0.38, 0.4);
        let thrust = envelope(t, 0.4, 0.5, 0.6, 1.0);
        p.r(J::ArmR, fwd(-0.6 * wind + 1.0 * thrust));
        p.r(J::ForeR, fwd(-0.6 * thrust));
        p.r(J::Torso, Quat::from_rotation_y(0.4 * wind - 0.3 * thrust) * bow(0.3 * thrust));
        p.o(J::Lift, Vec3::new(0.0, 0.0, 0.3 * thrust));
        return;
    }
    let rear = envelope(t, 0.0, 0.38, 0.4, 0.5);
    let fall = envelope(t, 0.42, 0.52, 0.6, 1.0);
    let gore = rig.style == Style::Unicorn;
    // Rear up on the hind legs, striking out, then come down hard.
    p.r(J::Body, bow(-0.55 * rear + 0.15 * fall));
    p.o(J::Body, Vec3::new(0.0, 0.25 * rear, -0.12 * rear));
    for (thigh, shin) in [(J::FL, J::FLs), (J::FR, J::FRs)] {
        p.r(thigh, fwd(1.0 * rear + (t * 30.0).sin() * 0.25 * rear));
        p.r(shin, fwd(-1.1 * rear));
    }
    p.r(J::Neck, bow(-0.35 * rear + if gore { 1.0 } else { 0.6 } * fall));
    p.r(J::Snout, bow(if gore { 0.5 } else { 0.2 } * fall));
    p.o(J::Lift, Vec3::new(0.0, 0.0, 0.32 * fall));
}

fn death_pose(p: &mut Pose, plan: Plan, d: f32, seed: f32) {
    let side = if seed > 5.0 { 1.0 } else { -1.0 };
    // A little variety from the seed, so no two bodies lie quite alike.
    let v = |k: u32| rand(u64::from(seed.to_bits()), k) - 0.5;
    match plan {
        Plan::Biped => {
            // Crumpled onto the back: tipped over along the rig's x axis and
            // rolled face-up, flat to the floor.
            p.rot[J::Lift as usize] =
                Quat::from_rotation_z(side * FRAC_PI_2 * d) * Quat::from_rotation_y(side * FRAC_PI_2 * d) * Quat::from_rotation_x(0.08 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.86 * d, 0.1 * d, 0.0);
            // Arms flung loose to the floor, elbows soft.
            p.r(J::ArmL, Quat::from_rotation_z((0.85 + v(1)) * d) * fwd(-0.35 * d));
            p.r(J::ArmR, Quat::from_rotation_z(-(0.55 + v(2)) * d) * fwd(-0.35 * d));
            // Twisted so the elbows fold along the floor, not up off it.
            p.r(J::ForeL, Quat::from_rotation_y(-FRAC_PI_2 * d) * fwd((0.45 + v(3) * 0.5) * d));
            p.r(J::ForeR, Quat::from_rotation_y(FRAC_PI_2 * d) * fwd((0.7 + v(4) * 0.5) * d));
            p.r(J::HandL, fwd(-0.4 * d));
            p.r(J::HandR, fwd(-0.4 * d));
            // Legs splayed, one knee fallen outward.
            p.r(J::LegL, Quat::from_rotation_z(0.22 * d) * Quat::from_rotation_y(0.5 * d) * fwd((0.35 + v(5) * 0.3) * d));
            p.r(J::ShinL, fwd(-0.55 * d));
            p.r(J::LegR, Quat::from_rotation_z(-0.12 * d) * Quat::from_rotation_y(-0.25 * d));
            p.r(J::ShinR, fwd(-0.12 * d));
            p.r(J::Torso, Quat::from_rotation_y(0.15 * side * d) * bow(-0.08 * d));
            // The head lolls to one side.
            p.r(J::Head, Quat::from_rotation_y(side * 1.0 * d) * bow(-0.3 * d));
        }
        Plan::Centaur => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.72 * d, 0.2 * d, 0.0);
            limp_legs(p, side, d);
            // The rider's half slumps over onto the floor.
            p.r(J::Torso, Quat::from_rotation_z(side * 0.9 * d) * bow(0.4 * d));
            p.r(J::ArmL, Quat::from_rotation_z(1.2 * d));
            p.r(J::ArmR, Quat::from_rotation_z(-0.3 * d) * fwd(0.6 * d));
            p.r(J::Head, Quat::from_rotation_y(side * 0.8 * d) * bow(0.4 * d));
        }
        Plan::Quadruped => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.72 * d, 0.2 * d, 0.0);
            limp_legs(p, side, d);
            // Neck stretched out along the ground, head resting on it.
            p.r(J::Neck, bow(1.05 * d) * Quat::from_rotation_z(-side * 0.15 * d));
            p.r(J::Snout, bow(-0.3 * d));
            p.r(J::Tail, fwd(0.6 * d) * Quat::from_rotation_z(-side * 0.6 * d));
        }
        Plan::Bird => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * 1.5 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.62 * d, 0.2 * d, 0.0);
            // Long legs sprawled, the neck laid flat with the head on the floor.
            p.r(J::LegL, fwd(0.6 * d) * Quat::from_rotation_z(-side * 0.35 * d));
            p.r(J::ShinL, fwd(-0.2 * d));
            p.r(J::LegR, fwd(-0.2 * d) * Quat::from_rotation_z(-side * 0.1 * d));
            p.r(J::ShinR, fwd(-0.5 * d));
            p.r(J::Neck, bow(1.25 * d) * Quat::from_rotation_z(-side * 0.35 * d));
            p.r(J::Head, bow(-0.4 * d));
        }
        Plan::Serpent | Plan::Floater => {}
    }
}

/// Four legs gone slack: the forelegs reach, the hind legs trail, and the
/// legs on top sag down toward the floor.
fn limp_legs(p: &mut Pose, side: f32, d: f32) {
    let sag = |left: bool| if (side > 0.0) == left { -side * 0.35 } else { -side * 0.08 };
    p.r(J::FL, Quat::from_rotation_z(sag(true) * d) * fwd(0.55 * d));
    p.r(J::FR, Quat::from_rotation_z(sag(false) * d) * fwd(0.3 * d));
    p.r(J::BL, Quat::from_rotation_z(sag(true) * d) * fwd(-0.35 * d));
    p.r(J::BR, Quat::from_rotation_z(sag(false) * d) * fwd(-0.55 * d));
    for shin in [J::FLs, J::FRs, J::BLs, J::BRs] {
        p.r(shin, fwd(-0.18 * d));
    }
    p.r(J::Body, Quat::from_rotation_x(0.05 * d));
}

/// Snakes and floaters place their pieces along paths rather than joints.
fn place_segments(rig: &Rig, a: &Anim, fun: f32, joints: &mut Query<&mut Transform, Without<Rig>>) {
    let (m, t) = a.action.map_or((None, 0.0), |(m, t)| (Some(m), t));
    let dead = a.dead.map(ease);
    let calm = (1.0 - a.speed).clamp(0.0, 1.0) * f32::from(u8::from(m.is_none() && a.dead.is_none()));
    let (w, gag, _) = antic(a);
    let spring = if gag == 0 { w * fun * calm } else { 0.0 };
    match rig.plan {
        Plan::Serpent => {
            let n = rig.segments.len();
            let amp = 0.05 + 0.1 * a.speed;
            let wave = a.walk * 2.5 + a.time * 1.5;
            let coil = envelope(t, 0.0, 0.35, 0.38, 0.45) * f32::from(u8::from(m.is_some()));
            let lunge = envelope(t, 0.4, 0.5, 0.6, 1.0) * f32::from(u8::from(m.is_some()));
            let d = dead.unwrap_or(0.0);
            let mut prev: Option<Vec3> = None;
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                let z = 0.6 - f * 0.068 + 0.35 * lunge - 0.15 * coil;
                let x = (wave - f * 0.5).sin() * amp * (0.3 + f / n as f32) * (1.0 - d) + d * (f * 0.4).sin() * 0.12;
                let front = (1.0 - f / 6.0).max(0.0);
                let rest = (1.0 - a.speed) * f32::from(u8::from(m.is_none()));
                let rear = every(a.time + a.seed * 3.7, 8.0, 3.0, 6.5) * rest;
                let y = 0.07
                    + front * (0.32 * coil + 0.18 * lunge + 0.22 * rear) * (1.0 - d)
                    + (a.time * 2.0 + f).sin() * 0.005;
                let mut at = Vec3::new(x, y, z);
                if spring > 0.0 {
                    // Coiled up like a spring, head on top, bouncing.
                    let turn = f * 0.9;
                    let bounce = (a.time * 7.0).sin().abs() * 0.12;
                    let coil = Vec3::new(turn.cos() * 0.13, 0.45 - f * 0.022 + bounce * (1.0 - f / n as f32), turn.sin() * 0.13);
                    at = at.lerp(coil, spring);
                }
                let dir = prev.map_or(Vec3::Z, |p: Vec3| (p - at).normalize_or(Vec3::Z));
                prev = Some(at);
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = at;
                    let face = if i == 0 { Vec3::Z } else { dir };
                    transform.rotation = Quat::from_rotation_arc(Vec3::Z, face) * Quat::from_rotation_z(d * 2.6);
                }
            }
            // The tongue flickers out every couple of seconds, and on a strike.
            if let Some(tongue) = rig.joint(J::Snout) {
                if let Ok(mut transform) = joints.get_mut(tongue) {
                    let cycle = (a.time * 0.6 + a.seed).fract();
                    let flick = if cycle < 0.18 { (cycle / 0.18 * PI * 3.0).sin().abs() } else { 0.0 };
                    let out = (flick + coil).min(1.0) * (1.0 - d);
                    transform.scale = Vec3::new(1.0, 1.0, out.max(0.01));
                }
            }
        }
        Plan::Floater => {
            let d = dead.unwrap_or(0.0);
            let flare = match m {
                Some(_) => envelope(t, 0.0, 0.4, 0.5, 1.0),
                None => 0.0,
            };
            let dash = match m {
                Some(m) if m.offensive() => envelope(t, 0.38, 0.48, 0.55, 1.0),
                _ => 0.0,
            };
            if let Some(core) = rig.joint(J::Core) {
                if let Ok(mut transform) = joints.get_mut(core) {
                    transform.translation =
                        Vec3::new(0.0, 1.1 + (a.time * 2.2 + a.seed).sin() * 0.08 - 0.9 * d, 0.45 * dash);
                    transform.scale = Vec3::splat((1.0 + 0.35 * flare) * (1.0 - 0.6 * d));
                    transform.rotation = Quat::from_rotation_y(a.time * 0.7);
                }
            }
            for (j, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
                if let Some(e) = rig.joint(j) {
                    if let Ok(mut transform) = joints.get_mut(e) {
                        transform.rotation = Quat::from_rotation_y(-side * 0.9 * dash - a.time * 0.7)
                            * Quat::from_rotation_z(side * (0.2 * (a.time * 1.3).sin() + 0.6 * flare));
                    }
                }
            }
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                let angle = a.time * (1.4 + f * 0.17) + f * TAU / 6.0;
                let r = (0.32 + 0.25 * flare) * (1.0 - d);
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = Vec3::new(
                        angle.cos() * r,
                        1.1 + (a.time * 3.0 + f).sin() * 0.25 - 0.9 * d,
                        angle.sin() * r + 0.45 * dash,
                    );
                    transform.scale = Vec3::splat(1.0 - d);
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Effects: bolts, flashes, shockwaves and wards.

#[derive(Component)]
pub struct Bolt {
    from: Vec3,
    to: Vec3,
    delay: f32,
    t: f32,
    wobble: f32,
    accent: Accent,
}

#[derive(Component)]
pub struct Fade {
    t: f32,
    life: f32,
    grow: f32,
    base: f32,
}

fn sphere(library: &mut PropLibrary, meshes: &mut Assets<Mesh>, accent: Accent) -> Model {
    library.get(format!("fx-sphere:{accent:?}"), meshes, move || {
        let mut p = Parts {
            accent: Some(accent),
            ..default()
        };
        p.glow.blob(Vec3::ZERO, Vec3::ONE, 0, 0.0, Vec3::ONE);
        p
    })
}

fn ring(library: &mut PropLibrary, meshes: &mut Assets<Mesh>, accent: Accent) -> Model {
    library.get(format!("fx-ring:{accent:?}"), meshes, move || {
        let mut p = Parts {
            accent: Some(accent),
            ..default()
        };
        p.glow.torus(Vec3::ZERO, Vec3::Y, 1.0, 0.035, 32, 3, Vec3::ONE);
        p
    })
}

pub fn bolt(commands: &mut Commands, library: &mut PropLibrary, meshes: &mut Assets<Mesh>, from: Vec3, to: Vec3, accent: Accent, delay: f32, wobble: f32) {
    let model = sphere(library, meshes, accent);
    commands
        .spawn((
            Bolt { from, to, delay, t: 0.0, wobble, accent },
            Transform::from_translation(from).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| {
            model.attach(p);
            p.spawn(PointLight {
                color: accent.light(),
                intensity: 12_000.0,
                range: 4.0,
                shadows_enabled: false,
                ..default()
            });
        });
}

pub fn flash(commands: &mut Commands, library: &mut PropLibrary, meshes: &mut Assets<Mesh>, at: Vec3, accent: Accent, size: f32, delay: f32) {
    let model = sphere(library, meshes, accent);
    commands
        .spawn((
            Fade { t: -delay, life: 0.3, grow: size, base: size * 0.2 },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

pub fn shockwave(commands: &mut Commands, library: &mut PropLibrary, meshes: &mut Assets<Mesh>, at: Vec3, accent: Accent, delay: f32) {
    let model = ring(library, meshes, accent);
    commands
        .spawn((
            Fade { t: -delay, life: 0.5, grow: 1.4, base: 0.15 },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

/// A ward: rings of light rising around a figure.
pub fn ward(commands: &mut Commands, library: &mut PropLibrary, meshes: &mut Assets<Mesh>, at: Vec3, accent: Accent) {
    for k in 0..3 {
        let model = ring(library, meshes, accent);
        commands
            .spawn((
                Fade { t: -(k as f32) * 0.12, life: 0.8, grow: 0.25, base: 0.5 },
                Transform::from_translation(at + Vec3::Y * (0.3 + k as f32 * 0.5)).with_scale(Vec3::ZERO),
                Visibility::default(),
            ))
            .with_children(|p| model.attach(p));
    }
}

pub fn effects(
    mut commands: Commands,
    time: Res<Time>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut bolts: Query<(Entity, &mut Bolt, &mut Transform), Without<Fade>>,
    mut fades: Query<(Entity, &mut Fade, &mut Transform), Without<Bolt>>,
) {
    let dt = time.delta_secs();
    for (e, mut bolt, mut transform) in &mut bolts {
        if bolt.delay > 0.0 {
            bolt.delay -= dt;
            continue;
        }
        bolt.t += dt / 0.32;
        let t = bolt.t.min(1.0);
        let side = (bolt.to - bolt.from).cross(Vec3::Y).normalize_or_zero();
        let arc = Vec3::Y * (t * PI).sin() * 0.25 + side * (t * TAU * 1.5).sin() * bolt.wobble;
        transform.translation = bolt.from.lerp(bolt.to, t) + arc;
        transform.scale = Vec3::splat(0.09 + 0.02 * (time.elapsed_secs() * 40.0).sin());
        if bolt.t >= 1.0 {
            commands.entity(e).despawn();
            flash(&mut commands, &mut library, &mut meshes, bolt.to, bolt.accent, 0.45, 0.0);
        }
    }
    for (e, mut fade, mut transform) in &mut fades {
        fade.t += dt;
        if fade.t < 0.0 {
            continue;
        }
        let k = fade.t / fade.life;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let size = fade.base + fade.grow * ease(k);
        let thin = 1.0 - k * k;
        transform.scale = Vec3::new(size, size * thin.max(0.05), size);
        if fade.base < 0.3 && fade.grow < 1.0 {
            transform.scale = Vec3::splat(size * thin.max(0.05));
        }
    }
}

/// A ring of light that hangs in the air and fades.
fn halo(commands: &mut Commands, library: &mut PropLibrary, meshes: &mut Assets<Mesh>, at: Vec3, accent: Accent, size: f32, delay: f32) {
    let model = ring(library, meshes, accent);
    commands
        .spawn((
            Fade { t: -delay, life: 0.7, grow: size, base: size * 0.4 },
            Transform::from_translation(at).with_scale(Vec3::ZERO),
            Visibility::default(),
        ))
        .with_children(|p| model.attach(p));
}

/// Stage a move's effects between a caster and its target (feet positions
/// and heights). Returns, when the target is struck, the delay until it
/// reacts to the blow or the spell's arrival.
#[allow(clippy::too_many_arguments)]
pub fn stage(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    action: Move,
    from: (Vec3, f32),
    to: Option<(Vec3, f32)>,
    missed: bool,
) -> Option<f32> {
    let impact = action.impact() * action.duration();
    let (caster, height) = from;
    let chest = |(at, h): (Vec3, f32)| at + Vec3::Y * h * 0.55;
    let toward = to.map_or(Vec3::Z, |(at, _)| (at - caster).normalize_or(Vec3::Z));
    let hand = caster + Vec3::Y * height * 0.7 + toward * 0.35;
    match action {
        Move::Guard(accent) => {
            ward(commands, library, meshes, caster, accent);
            return None;
        }
        Move::Focus => {
            halo(commands, library, meshes, caster + Vec3::Y * height * 0.92, Accent::Teal, 0.25, 0.2);
            halo(commands, library, meshes, caster + Vec3::Y * height * 0.92, Accent::Teal, 0.25, 0.55);
            return None;
        }
        Move::Step => {
            shockwave(commands, library, meshes, caster + Vec3::Y * 0.04, Accent::White, impact);
            return None;
        }
        Move::Mend => {
            ward(commands, library, meshes, caster, Accent::Teal);
            for k in 0..3 {
                flash(commands, library, meshes, caster + Vec3::Y * (0.5 + 0.12 * k as f32), Accent::Teal, 0.18, impact + k as f32 * 0.2);
            }
            return None;
        }
        Move::Fear => {
            for k in 0..3 {
                halo(commands, library, meshes, caster + Vec3::Y * height * 0.85, Accent::Violet, 0.9 + k as f32 * 0.4, impact + k as f32 * 0.1);
            }
        }
        _ => {}
    }
    let target = to?;
    let travel = match action {
        Move::Cast(accent) => {
            bolt(commands, library, meshes, hand, chest(target), accent, impact, 0.0);
            0.32
        }
        Move::Hex => {
            bolt(commands, library, meshes, hand, chest(target), Accent::Violet, impact, 0.2);
            0.32
        }
        Move::Drain => {
            // The life leaves the target and flows back into the caster.
            for k in 0..3 {
                bolt(commands, library, meshes, chest(target), hand, Accent::Ember, impact + k as f32 * 0.12, 0.12);
            }
            0.0
        }
        _ => 0.0,
    };
    if !missed {
        let (accent, size) = match action {
            Move::Touch(accent) => (accent, 0.55),
            Move::Heavy => (Accent::Gold, 0.45),
            Move::Bash | Move::Shove => (Accent::Gold, 0.35),
            Move::Cast(_) | Move::Hex | Move::Drain | Move::Fear => (Accent::White, 0.0),
            _ => (Accent::White, 0.32),
        };
        if size > 0.0 {
            flash(commands, library, meshes, chest(target), accent, size, impact);
        }
        if action == Move::Heavy {
            shockwave(commands, library, meshes, target.0 + Vec3::Y * 0.05, Accent::Gold, impact);
        }
    }
    Some(impact + travel)
}

/// A corpse on the floor: the dead creature's own model, held in its death
/// pose. `name` is the item's name, as in "emu corpse".
pub fn spawn_corpse(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    name: &str,
    at: Vec3,
    yaw: f32,
    individual: u64,
) -> Entity {
    let creature = corpse_of(name);
    let look = Look::of(creature, false, individual);
    let mut anim = Anim::corpse(seed(creature));
    anim.yaw = yaw;
    let entity = commands.spawn((anim, Transform::from_translation(at), Visibility::default())).id();
    let rig = spawn(commands, library, meshes, entity, &look);
    commands.entity(entity).insert(rig);
    entity
}

/// Lights and glowing parts dimmed on the dead, to restore if they rise.
#[derive(Component)]
pub struct Snuffed(Vec<(Entity, f32)>);

/// Put out the glow of the dead: glowing parts fade to embers and any light
/// they carried goes out. Bring it back if the creature stirs again.
pub fn snuff(
    mut commands: Commands,
    library: Option<Res<PropLibrary>>,
    rigs: Query<(Entity, &Anim, Option<&Snuffed>), With<Rig>>,
    children: Query<&Children>,
    mut materials: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut lights: Query<&mut PointLight>,
) {
    let Some(library) = library else {
        return;
    };
    for (entity, anim, snuffed) in &rigs {
        let dead = anim.dead.is_some_and(|d| d > 0.3);
        match (dead, snuffed) {
            (true, None) => {
                let mut put_out = Vec::new();
                for e in children.iter_descendants(entity) {
                    if let Ok(mut material) = materials.get_mut(e) {
                        if let Some(dim) = library.materials.dim(&material.0) {
                            material.0 = dim;
                        }
                    }
                    if let Ok(mut light) = lights.get_mut(e) {
                        put_out.push((e, light.intensity));
                        light.intensity = 0.0;
                    }
                }
                commands.entity(entity).insert(Snuffed(put_out));
            }
            (false, Some(snuffed)) => {
                for e in children.iter_descendants(entity) {
                    if let Ok(mut material) = materials.get_mut(e) {
                        if let Some(bright) = library.materials.undim(&material.0) {
                            material.0 = bright;
                        }
                    }
                }
                for (e, intensity) in &snuffed.0 {
                    if let Ok(mut light) = lights.get_mut(*e) {
                        light.intensity = *intensity;
                    }
                }
                commands.entity(entity).remove::<Snuffed>();
            }
            _ => {}
        }
    }
}

/// A corpse's creature: the name before "corpse", as in "emu corpse".
pub fn corpse_of(name: &str) -> &str {
    name.trim_end_matches(" corpse").trim_end_matches(" remains").trim()
}

/// A seed for one individual, from the server's reference to it. Stable for
/// as long as this observer sees it; the server will disclose a shared
/// appearance seed later (see docs/plans/presentation.md).
pub fn individual(id: &impl std::fmt::Debug) -> u64 {
    hash(&format!("{id:?}"))
}

/// A seed that stays the same for a creature of this name.
pub fn seed(name: &str) -> u64 {
    hash(name)
}
