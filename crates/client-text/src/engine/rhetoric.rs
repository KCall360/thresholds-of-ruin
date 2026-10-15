//! How a blow is told: who struck whom, with what, and how it landed, in
//! words that vary from one blow to the next.
//!
//! Only what is disclosed shapes the telling: the ability's name and whether
//! it is a basic attack, the character's own wielded weapon and its damage
//! category, and the outcome. A figure's weapon is never guessed at.
use tor_protocol::{AbilityOutcome, DamageType};

/// Everything a blow's sentence is made from. `attacker` and `target` are
/// already worded for the passage: "you", "the ruin scout", "it".
pub struct Blow<'a> {
    pub attacker: &'a str,
    pub target: &'a str,
    pub by_me: bool,
    pub at_me: bool,
    /// The ability's disclosed name: "Attack", "Rattling Blow".
    pub ability: &'a str,
    /// A plain attack rather than a technique.
    pub basic: bool,
    /// The character's own weapon, when it struck with one: its name and
    /// damage category.
    pub weapon: Option<(&'a str, Option<DamageType>)>,
    pub outcome: AbilityOutcome,
    /// Which of the alternatives to use.
    pub variant: u32,
    /// Which strike of a sequence, when it's a later one: " (strike 2)".
    pub ordinal: &'a str,
}

/// Self-targeted techniques whose name reads as what's done: "Brace for
/// Impact" is "you brace for impact".
const DOING: &[&str] = &[
    "brace", "bind", "steady", "focus", "guard", "ward", "rally", "gather",
];

fn pick<'t>(choices: &[&'t str], variant: u32) -> &'t str {
    choices[usize::try_from(variant).unwrap_or(0) % choices.len()]
}

/// The blow as a clause, without its final full stop.
pub fn tell(b: &Blow) -> String {
    let (a, t) = (b.attacker, b.target);
    let template = if b.by_me && b.at_me {
        return own(b);
    } else if b.basic && b.by_me {
        match (b.weapon, b.outcome) {
            (Some((_, Some(DamageType::Keen))), AbilityOutcome::Applied) => pick(
                &[
                    "you slash at {t} with your {w} and connect",
                    "your {w} bites into {t}",
                    "you cut into {t} with your {w}",
                ],
                b.variant,
            ),
            (Some((_, Some(DamageType::Impact))), AbilityOutcome::Applied) => pick(
                &[
                    "you bring your {w} crashing down on {t}",
                    "your {w} smashes into {t}",
                    "you batter {t} with your {w}",
                ],
                b.variant,
            ),
            (Some(_), AbilityOutcome::Applied) => pick(
                &[
                    "you strike {t} with your {w}",
                    "your {w} lands on {t}",
                    "you land a blow on {t} with your {w}",
                ],
                b.variant,
            ),
            (Some(_), AbilityOutcome::Miss) => pick(
                &[
                    "you swing your {w} at {t}, but miss",
                    "{t} slips aside from your {w}",
                    "your {w} cuts only air",
                ],
                b.variant,
            ),
            (Some(_), AbilityOutcome::Unaffected) => pick(
                &[
                    "your {w} strikes {t}, but seems to do no harm",
                    "you hit {t} with your {w}, to no visible effect",
                ],
                b.variant,
            ),
            (None, AbilityOutcome::Applied) => pick(
                &[
                    "you strike {t}",
                    "you land a blow on {t}",
                    "you hit {t} hard",
                ],
                b.variant,
            ),
            (None, AbilityOutcome::Miss) => pick(
                &["you swing at {t}, but miss", "{t} dodges your blow"],
                b.variant,
            ),
            (None, AbilityOutcome::Unaffected) => "you strike {t}, but it seems unharmed",
        }
    } else if b.basic && b.at_me {
        match b.outcome {
            AbilityOutcome::Applied => pick(
                &[
                    "{a} strikes you",
                    "{a} lands a blow on you",
                    "{a} catches you with a blow",
                    "{a} hits you",
                ],
                b.variant,
            ),
            AbilityOutcome::Miss => pick(
                &[
                    "{a} lunges at you, but misses",
                    "{a} attacks, but you avoid the blow",
                    "{a} swings at you and misses",
                ],
                b.variant,
            ),
            AbilityOutcome::Unaffected => "{a} strikes you, but you shrug it off",
        }
    } else if b.basic {
        match b.outcome {
            AbilityOutcome::Applied => "{a} strikes {t}",
            AbilityOutcome::Miss => "{a} attacks {t}, but misses",
            AbilityOutcome::Unaffected => "{a} strikes {t}, to no effect",
        }
    } else if b.by_me {
        match b.outcome {
            AbilityOutcome::Applied => pick(
                &["you strike {t} with {n}", "you turn {n} on {t}"],
                b.variant,
            ),
            AbilityOutcome::Miss => "you try {n} on {t}, but miss",
            AbilityOutcome::Unaffected => "you use {n} on {t}",
        }
    } else if b.attacker == b.target {
        "{a} uses {n}"
    } else if b.at_me {
        match b.outcome {
            AbilityOutcome::Applied => "{a} turns {n} on you",
            AbilityOutcome::Miss => "{a} tries {n} on you, but misses",
            // Riders the marker can't summarize may still have landed.
            AbilityOutcome::Unaffected => "{a} uses {n} on you",
        }
    } else {
        match b.outcome {
            AbilityOutcome::Miss => "{a} tries {n} on {t}, but misses",
            _ => "{a} uses {n} on {t}",
        }
    };
    let told = fill(template, b).replace("{a}", a).replace("{t}", t);
    if template.contains("{n}") {
        told
    } else {
        format!("{told}{}", b.ordinal)
    }
}

fn fill(template: &str, b: &Blow) -> String {
    template
        .replace("{w}", b.weapon.map_or("weapon", |(w, _)| w))
        .replace("{n}", &format!("{}{}", b.ability, b.ordinal))
}

/// A technique the character uses on themself.
fn own(b: &Blow) -> String {
    let lower = b.ability.to_lowercase();
    let first = lower.split_whitespace().next().unwrap_or("");
    let doing = !b.ability.contains('(') && DOING.contains(&first);
    match (b.outcome, doing) {
        (AbilityOutcome::Miss, _) => format!("you try {}{}, but it fails", b.ability, b.ordinal),
        (_, true) => format!("you {lower}{}", b.ordinal),
        (_, false) => format!("you use {}{}", b.ability, b.ordinal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blow(by_me: bool, at_me: bool, basic: bool, outcome: AbilityOutcome) -> Blow<'static> {
        Blow {
            attacker: if by_me { "you" } else { "the ruin scout" },
            target: if at_me { "you" } else { "the ruin scout" },
            by_me,
            at_me,
            ability: if basic { "Attack" } else { "Rattling Blow" },
            basic,
            weapon: by_me.then_some(("iron greatsword", Some(DamageType::Keen))),
            outcome,
            variant: 0,
            ordinal: "",
        }
    }

    #[test]
    fn a_wielded_weapon_is_named_and_its_cut_told() {
        assert_eq!(
            tell(&blow(true, false, true, AbilityOutcome::Applied)),
            "you slash at the ruin scout with your iron greatsword and connect"
        );
        assert_eq!(
            tell(&blow(true, false, true, AbilityOutcome::Miss)),
            "you swing your iron greatsword at the ruin scout, but miss"
        );
    }

    #[test]
    fn blows_vary_from_one_to_the_next() {
        let told: std::collections::BTreeSet<String> = (0..3)
            .map(|variant| {
                tell(&Blow {
                    variant,
                    ..blow(false, true, true, AbilityOutcome::Applied)
                })
            })
            .collect();
        assert_eq!(told.len(), 3);
    }

    #[test]
    fn techniques_are_named_and_self_help_reads_as_doing() {
        assert_eq!(
            tell(&blow(true, false, false, AbilityOutcome::Applied)),
            "you strike the ruin scout with Rattling Blow"
        );
        let brace = Blow {
            ability: "Brace for Impact",
            ..blow(true, true, false, AbilityOutcome::Applied)
        };
        assert_eq!(tell(&brace), "you brace for impact");
        let shell = Blow {
            ability: "Shell (Impact)",
            ..blow(true, true, false, AbilityOutcome::Applied)
        };
        assert_eq!(tell(&shell), "you use Shell (Impact)");
        assert_eq!(
            tell(&blow(false, true, false, AbilityOutcome::Miss)),
            "the ruin scout tries Rattling Blow on you, but misses"
        );
    }
}
