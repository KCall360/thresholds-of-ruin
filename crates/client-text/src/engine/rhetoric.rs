//! How a blow is told: who struck whom, with what, and how it landed, in
//! words that vary from one blow to the next.
//!
//! Only what is disclosed shapes the telling: the ability's name and whether
//! it is a basic attack, and the outcome. Current inventory never binds a
//! weapon to an event; no source or damage strength is guessed.
use tor_protocol::AbilityOutcome;

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
        match b.outcome {
            AbilityOutcome::Applied => pick(
                &[
                    "you strike {t}",
                    "you land a blow on {t}",
                    "your attack connects with {t}",
                ],
                b.variant,
            ),
            AbilityOutcome::Miss => pick(
                &["you swing at {t}, but miss", "{t} dodges your blow"],
                b.variant,
            ),
            AbilityOutcome::Unaffected => "you strike at {t}",
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
            AbilityOutcome::Unaffected => "{a} attacks you",
        }
    } else if b.basic {
        match b.outcome {
            AbilityOutcome::Applied => "{a} strikes {t}",
            AbilityOutcome::Miss => "{a} attacks {t}, but misses",
            AbilityOutcome::Unaffected => "{a} attacks {t}",
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
    template.replace("{n}", &format!("{}{}", b.ability, b.ordinal))
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
            outcome,
            variant: 0,
            ordinal: "",
        }
    }

    #[test]
    fn a_basic_attack_is_told_without_inventing_a_weapon() {
        assert_eq!(
            tell(&blow(true, false, true, AbilityOutcome::Applied)),
            "you strike the ruin scout"
        );
        assert_eq!(
            tell(&blow(true, false, true, AbilityOutcome::Miss)),
            "you swing at the ruin scout, but miss"
        );
    }

    #[test]
    fn unaffected_primary_never_claims_absent_harm_or_effects() {
        for (by_me, at_me) in [(true, false), (false, true), (false, false)] {
            for variant in 0..3 {
                let mut b = blow(by_me, at_me, true, AbilityOutcome::Unaffected);
                b.variant = variant;
                let told = tell(&b);
                for claim in [
                    "no harm",
                    "no visible effect",
                    "unharmed",
                    "shrug",
                    "no effect",
                ] {
                    assert!(
                        !told.contains(claim),
                        "primary alone cannot support: {told}"
                    );
                }
            }
        }
    }

    #[test]
    fn an_applied_primary_alone_never_claims_damage_strength() {
        for variant in 0..3 {
            let mut b = blow(true, false, true, AbilityOutcome::Applied);
            b.variant = variant;
            let told = tell(&b);
            assert!(
                !told.contains("hard"),
                "a primary can apply a non-damage effect: {told}"
            );
        }
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
