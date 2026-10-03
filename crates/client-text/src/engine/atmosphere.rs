//! Atmosphere: the mood of a place, its air, smell and sound.
//!
//! Atmosphere colours descriptions and answers `smell` and `listen`, but has
//! no effect on play and claims nothing the game would act on. Each place gets
//! one *theme*, chosen by what it's made of, so its mood word, air, smell and
//! sound agree with each other ("a damp chamber" smells of wet stone, not of
//! dust). Every choice is a hash of the place's key, so a place reads the same
//! every time it's described, and neighbouring places rarely read alike.

use super::place::Form;

/// What a place is mostly made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fabric {
    Stone,
    Marble,
    Timber,
    Earth,
    /// Nothing seen to say: open ground in raw regions.
    Unknown,
}

impl Fabric {
    /// The fabric that surface words name: "dressed stone", "packed earth".
    pub fn of<'a>(walls: impl IntoIterator<Item = &'a str>, floor: Option<&'a str>) -> Fabric {
        let words: Vec<String> = walls
            .into_iter()
            .chain(floor)
            .map(str::to_lowercase)
            .collect();
        let any = |needles: &[&str]| words.iter().any(|w| needles.iter().any(|n| w.contains(n)));
        if any(&["marble"]) {
            Fabric::Marble
        } else if any(&["stone", "rock", "granite", "brick", "slate"]) {
            Fabric::Stone
        } else if any(&["wood", "timber", "plank", "board"]) {
            Fabric::Timber
        } else if any(&["earth", "dirt", "loam", "soil", "clay", "mud"]) {
            Fabric::Earth
        } else {
            Fabric::Unknown
        }
    }
}

/// A coherent mood: words and sentences that belong together.
struct Theme {
    moods: &'static [&'static str],
    air: &'static [&'static str],
    smell: &'static [&'static str],
    sound: &'static [&'static str],
}

const STONE: &[Theme] = &[
    Theme {
        moods: &["dusty", "dry"],
        air: &[
            "The air is dry and still, heavy with old dust.",
            "Motes of dust hang in the still air, stirring as you move.",
        ],
        smell: &[
            "The air smells of dust and dry stone.",
            "A dry, papery smell of old dust hangs here.",
        ],
        sound: &[
            "Your breathing sounds loud in the dry stillness.",
            "Your footsteps fall soft and muffled in the dust.",
        ],
    },
    Theme {
        moods: &["damp", "dank"],
        air: &[
            "A damp chill clings to the stones.",
            "The air is clammy, and the stone is cold and slick with moisture.",
        ],
        smell: &[
            "The air smells of wet stone and cold water.",
            "A dank, mossy smell hangs in the air.",
        ],
        sound: &[
            "Somewhere out of sight, water drips slowly.",
            "A slow drip sounds faintly from somewhere far off.",
        ],
    },
    Theme {
        moods: &["echoing", "hollow"],
        air: &[
            "Every sound you make comes back to you from the stone.",
            "Your footsteps ring back at you, sharp and close.",
        ],
        smell: &[
            "The air smells clean and cold, of bare stone.",
            "There is a faint, clean smell of cut stone.",
        ],
        sound: &[
            "Your own breathing echoes softly back to you.",
            "Each small sound you make returns a moment later, fainter.",
        ],
    },
    Theme {
        moods: &["gloomy", "shadowed", "dim"],
        air: &[
            "Shadows gather thickly in the corners.",
            "The light seems thin here, and shadows pool along the stone.",
        ],
        smell: &[
            "The air is stale and unmoving.",
            "There is a flat, stale smell, as of air long undisturbed.",
        ],
        sound: &[
            "The silence here feels heavy.",
            "A heavy hush presses in around you.",
        ],
    },
    Theme {
        moods: &["drafty", "chill"],
        air: &[
            "A faint draught stirs the air, cool against your face.",
            "Cool air moves past you in slow, uneven breaths.",
        ],
        smell: &[
            "The draught carries a faint, earthy smell from somewhere beyond.",
            "The moving air smells cold and clean.",
        ],
        sound: &[
            "Air sighs faintly through the stonework.",
            "A low, breathy whistle rises and fades as the air moves.",
        ],
    },
    Theme {
        moods: &["musty", "airless"],
        air: &[
            "The air is stale, as if nothing has stirred here in a long time.",
            "The air hangs close and stale.",
        ],
        smell: &[
            "A musty, shut-in smell hangs in the air.",
            "The air smells of mould and old stone.",
        ],
        sound: &[
            "No sound disturbs the quiet.",
            "There is no sound at all but your own.",
        ],
    },
    Theme {
        moods: &["quiet", "hushed", "silent"],
        air: &[
            "The air is cool and perfectly still.",
            "A deep hush hangs over everything.",
        ],
        smell: &[
            "The air smells of nothing but cool stone.",
            "There is only the faint, clean smell of stone.",
        ],
        sound: &[
            "It is utterly quiet.",
            "The quiet is so complete that you can hear your own heartbeat.",
        ],
    },
    Theme {
        moods: &["cold", "chilly"],
        air: &[
            "Cold seeps up from the floor and into your bones.",
            "Your breath clouds faintly in the cold air.",
        ],
        smell: &[
            "The cold air has a sharp, mineral tang.",
            "The air smells cold and sharp, like flint.",
        ],
        sound: &[
            "A faint, ringing silence fills your ears.",
            "The cold seems to deaden every sound.",
        ],
    },
    Theme {
        moods: &["solemn", "sombre"],
        air: &[
            "A solemn stillness hangs over the place.",
            "There is a stillness here like that of a chapel long abandoned.",
        ],
        smell: &[
            "The air smells of old stone and older silence.",
            "A faint, cold smell of old stone lingers.",
        ],
        sound: &[
            "Your footsteps sound like an intrusion here.",
            "The silence seems to listen back.",
        ],
    },
];

const MARBLE: &[Theme] = &[
    Theme {
        moods: &["cool", "still"],
        air: &[
            "The polished stone is cool, and the air above it perfectly still.",
            "The smooth stone gives back a faint, cold sheen.",
        ],
        smell: &[
            "The air smells of nothing at all, as clean as the stone.",
            "The air is cool and faintly mineral.",
        ],
        sound: &[
            "Your footsteps ring sharply on the polished stone.",
            "Every sound carries crisply across the smooth stone.",
        ],
    },
    Theme {
        moods: &["stately", "echoing"],
        air: &[
            "There is a cold grandeur to the place.",
            "The pale stone lends the place a quiet dignity.",
        ],
        smell: &[
            "A faint, cool smell of polished stone hangs in the air.",
            "The air is clean and cold.",
        ],
        sound: &[
            "Sounds carry far here, and return softened.",
            "Your breathing echoes faintly off the smooth stone.",
        ],
    },
];

const TIMBER: &[Theme] = &[
    Theme {
        moods: &["creaking", "dusty"],
        air: &[
            "Old boards creak faintly as the air shifts.",
            "The air is dry, and the old wood ticks as it settles.",
        ],
        smell: &[
            "You smell old wood and dust.",
            "The dry aroma of seasoned timber lingers here.",
        ],
        sound: &[
            "The timbers creak and settle around you.",
            "Somewhere a beam ticks softly as it settles.",
        ],
    },
    Theme {
        moods: &["close", "dim"],
        air: &[
            "The air is close and warm, and smells of resin.",
            "The wood seems to hold the warmth in, and the air is close.",
        ],
        smell: &[
            "A faint smell of pine resin lingers in the wood.",
            "The air smells warm and woody.",
        ],
        sound: &[
            "The wood muffles every sound.",
            "It is quiet, but for the faint creak of old wood.",
        ],
    },
    Theme {
        moods: &["damp", "musty"],
        air: &[
            "The air is damp, and smells of wet wood.",
            "A damp, musty air hangs among the timbers.",
        ],
        smell: &[
            "The sour smell of damp wood hangs in the air.",
            "You smell mildew and old, damp wood.",
        ],
        sound: &[
            "Somewhere the wood creaks softly.",
            "Water drips somewhere, slow and patient.",
        ],
    },
];

const EARTH: &[Theme] = &[
    Theme {
        moods: &["earthy", "damp"],
        air: &[
            "The scent of cool earth hangs heavy in the air.",
            "A damp, loamy hush settles over everything.",
        ],
        smell: &[
            "The rich, damp smell of earth and loam fills the air.",
            "You smell wet soil and roots.",
        ],
        sound: &[
            "Now and then a little earth trickles down somewhere nearby.",
            "The earth swallows every sound.",
        ],
    },
    Theme {
        moods: &["close", "musty"],
        air: &[
            "The air is close and heavy, and tastes of soil.",
            "The air is thick, and the earth seems to press in around you.",
        ],
        smell: &[
            "You smell soil, roots and cold stone.",
            "A musty, earthen smell fills your nose.",
        ],
        sound: &[
            "The earth deadens every sound to a murmur.",
            "Your breathing sounds very close in the heavy air.",
        ],
    },
    Theme {
        moods: &["cold", "dank"],
        air: &[
            "A cold, damp breath rises from the ground.",
            "The cold here is the deep, patient cold of the earth.",
        ],
        smell: &[
            "The air smells of cold mud.",
            "A dank smell of wet earth hangs in the air.",
        ],
        sound: &[
            "Water drips somewhere in the dark.",
            "A slow trickle of water sounds faintly, then stops.",
        ],
    },
];

/// Open ground, and places nothing is seen of: no walls or corners, and
/// nothing about stone.
const UNKNOWN: &[Theme] = &[
    Theme {
        moods: &["quiet", "still"],
        air: &[
            "The air is cool and still.",
            "Nothing moves in the still air.",
        ],
        smell: &[
            "The air carries no distinct scent.",
            "The air smells faintly cool, and of nothing else.",
        ],
        sound: &["All is quiet.", "It is very quiet."],
    },
    Theme {
        moods: &["drafty", "airy"],
        air: &["A faint breeze moves the air.", "Cool air drifts past you."],
        smell: &[
            "The moving air smells clean.",
            "The breeze carries no smell you can name.",
        ],
        sound: &[
            "The air whispers faintly around you.",
            "There is only the faint sigh of moving air.",
        ],
    },
    Theme {
        moods: &["dim", "shadowed"],
        air: &[
            "Shadows lie heavy all around.",
            "The light is thin and grey.",
        ],
        smell: &["The air is stale.", "The air smells flat and stale."],
        sound: &["A heavy silence hangs here.", "Nothing stirs."],
    },
];

/// A narrow passage feels close, whatever it's made of.
const NARROW: &[&str] = &[
    "The walls press close on either side.",
    "There is barely room to turn around.",
];

/// The atmosphere of one place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atmosphere {
    /// The mood word for its name: "a small, damp chamber".
    pub mood: &'static str,
    /// The sentences that colour its description.
    pub description: Vec<&'static str>,
    /// What `smell` notices.
    pub smell: &'static str,
    /// What `listen` hears.
    pub sound: &'static str,
}

/// A stable number for a key and one choice made about it, so that choices
/// are independent of each other: two places with the same mood word need
/// not share their smell.
fn choose(key: &str, choice: &str, count: usize) -> usize {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in key.bytes().chain([0]).chain(choice.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    // Finish with a mix so short keys that differ in one byte spread well.
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    (hash % count.max(1) as u64) as usize
}

/// The atmosphere of the place with this key.
pub fn of(key: &str, fabric: Fabric, form: Form, narrow: bool) -> Atmosphere {
    // Open ground has no corners or walls to colour, whatever its floor.
    let themes = match fabric {
        _ if form == Form::Open => UNKNOWN,
        Fabric::Stone => STONE,
        Fabric::Marble => MARBLE,
        Fabric::Timber => TIMBER,
        Fabric::Earth => EARTH,
        Fabric::Unknown => UNKNOWN,
    };
    let theme = &themes[choose(key, "theme", themes.len())];
    let pick = |name: &str, list: &'static [&'static str]| list[choose(key, name, list.len())];
    let mood = pick("mood", theme.moods);
    let smell = pick("smell", theme.smell);
    let sound = pick("sound", theme.sound);
    let mut description = vec![pick("air", theme.air)];
    if narrow && form == Form::Passage && choose(key, "narrow", 2) == 0 {
        description.push(pick("close", NARROW));
    }
    // Some places are known by their smell or sound as well.
    match choose(key, "sense", 3) {
        0 => description.push(smell),
        1 => description.push(sound),
        _ => {}
    }
    Atmosphere {
        mood,
        description,
        smell,
        sound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn a_place_always_has_the_same_atmosphere() {
        let a = of("cell-17", Fabric::Stone, Form::Chamber, false);
        assert_eq!(a, of("cell-17", Fabric::Stone, Form::Chamber, false));
    }

    #[test]
    fn places_vary_and_their_choices_are_independent() {
        let places: Vec<Atmosphere> = (0..200)
            .map(|i| of(&format!("place-{i}"), Fabric::Stone, Form::Chamber, false))
            .collect();
        let moods: BTreeSet<_> = places.iter().map(|a| a.mood).collect();
        let openings: BTreeSet<_> = places.iter().map(|a| a.description[0]).collect();
        let whole: BTreeSet<_> = places
            .iter()
            .map(|a| (a.mood, a.description.clone()))
            .collect();
        assert!(moods.len() >= 15, "{moods:?}");
        assert_eq!(openings.len(), STONE.len() * 2);
        assert!(whole.len() >= 60, "{}", whole.len());
    }

    #[test]
    fn a_theme_holds_together() {
        // Whatever is chosen, the mood, smell and sound come from one theme.
        for i in 0..100 {
            let a = of(&format!("k{i}"), Fabric::Stone, Form::Chamber, false);
            let theme = STONE.iter().find(|t| t.moods.contains(&a.mood)).unwrap();
            assert!(theme.smell.contains(&a.smell));
            assert!(theme.sound.contains(&a.sound));
            assert!(theme.air.contains(&a.description[0]));
        }
    }

    #[test]
    fn fabric_comes_from_surface_words() {
        assert_eq!(
            Fabric::of(["dressed stone"], Some("flagstone")),
            Fabric::Stone
        );
        assert_eq!(Fabric::of(["polished marble"], None), Fabric::Marble);
        assert_eq!(Fabric::of([], Some("packed earth")), Fabric::Earth);
        assert_eq!(Fabric::of(["oak planks"], None), Fabric::Timber);
        assert_eq!(Fabric::of([], None), Fabric::Unknown);
    }

    #[test]
    fn mood_words_take_the_right_article() {
        for themes in [STONE, MARBLE, TIMBER, EARTH, UNKNOWN] {
            for theme in themes {
                for mood in theme.moods {
                    let phrase = super::super::prose::indefinite(&format!("{mood} chamber"));
                    assert!(
                        !phrase.starts_with("a e") && !phrase.starts_with("a o"),
                        "{phrase}"
                    );
                }
            }
        }
    }
}
