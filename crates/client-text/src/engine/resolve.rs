//! Noun phrases to referents in the current scene.
use std::collections::BTreeMap;

use crate::parser::{NounPhrase, Pronoun};

use super::{
    prose,
    scene::{distance, Key, Kind, Referent, Scene},
};

/// What a verb would rather act on. Candidates in a better tier hide those in
/// worse ones, so `take scout` means the scout's corpse, not the scout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Domain {
    /// Anything, things before figures before doors.
    Any,
    /// Things lying here, then carried things, then anything.
    Ground,
    /// Carried things, then anything.
    Carried,
    Figures,
    Doors,
}

impl Domain {
    fn tier(self, r: &Referent) -> u8 {
        let thing = r.is(Kind::Thing);
        match self {
            Domain::Any => match r.kind {
                Kind::Thing | Kind::Figure => 0,
                Kind::Door => 1,
                Kind::Surface | Kind::Me => 2,
            },
            Domain::Ground => match (thing, r.carried) {
                (true, false) => 0,
                (true, true) => 1,
                _ => 2,
            },
            Domain::Carried => match (thing, r.carried) {
                (true, true) => 0,
                (true, false) => 1,
                _ => 2,
            },
            Domain::Figures => u8::from(!r.is(Kind::Figure)) * 2,
            Domain::Doors => u8::from(!r.is(Kind::Door)) * 2,
        }
    }

    /// Whether `all` includes it.
    fn includes(self, r: &Referent) -> bool {
        match self {
            Domain::Ground => r.is(Kind::Thing) && !r.carried,
            Domain::Carried => r.is(Kind::Thing) && r.carried,
            Domain::Figures => r.is(Kind::Figure),
            Domain::Doors => r.is(Kind::Door),
            Domain::Any => r.is(Kind::Thing),
        }
    }
}

/// What pronouns refer to, and the last thing the player or the narration
/// mentioned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Referents {
    pub it: Option<Key>,
    pub them: Vec<Key>,
    pub him: Option<Key>,
    pub her: Option<Key>,
}

impl Referents {
    pub fn mention(&mut self, r: &Referent) {
        match r.kind {
            Kind::Me => {}
            Kind::Figure => {
                self.him = Some(r.key);
                self.her = Some(r.key);
                self.it = Some(r.key);
            }
            _ => {
                if r.quantity > 1 {
                    self.them = vec![r.key];
                }
                self.it = Some(r.key);
            }
        }
    }

    pub fn mention_many(&mut self, keys: &[Key]) {
        if keys.len() > 1 {
            self.them = keys.to_vec();
        } else if let Some(key) = keys.first() {
            self.it = Some(*key);
        }
    }

    /// Forget referents no longer anywhere, as after a rewind.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolution {
    One(Key),
    /// A plural or `all`, in scene order.
    Many(Vec<Key>),
    /// Distinguishable candidates; the player must choose.
    Ask(Vec<Key>),
    /// Nothing fits; the text says why.
    Missing(String),
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Fit {
    /// A word of its name used as the noun: "scout" for the scout's corpse.
    Word,
    Plural,
    Head,
}

fn fit(np: &NounPhrase, r: &Referent) -> Option<Fit> {
    if !np.adjectives.iter().all(|a| r.words.contains(a)) {
        return None;
    }
    let Some(head) = &np.head else {
        // "the copper one": adjectives alone.
        return (!np.adjectives.is_empty()).then_some(Fit::Head);
    };
    if r.heads.contains(head) {
        Some(Fit::Head)
    } else if r.heads.iter().any(|h| prose::is_plural_of(head, h)) {
        Some(Fit::Plural)
    } else if r.words.contains(head) {
        Some(Fit::Word)
    } else {
        None
    }
}

/// The words of a phrase as the player typed them, without determiners.
pub fn phrase(np: &NounPhrase) -> String {
    np.adjectives
        .iter()
        .chain(np.head.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Bind a noun phrase to referents in `scene`.
pub fn resolve(
    np: &NounPhrase,
    scene: &Scene,
    referents: &Referents,
    domain: Domain,
) -> Resolution {
    if let Some(pronoun) = np.pronoun {
        return pronoun_referents(pronoun, scene, referents);
    }
    if np.all {
        // "except" leaves out everything it names, not one of a kind.
        let except: Vec<Key> = match np.except.as_deref() {
            Some(except) if except.pronoun.is_some() => {
                match resolve(except, scene, referents, domain) {
                    Resolution::One(key) => vec![key],
                    Resolution::Many(keys) | Resolution::Ask(keys) => keys,
                    Resolution::Missing(text) => return Resolution::Missing(text),
                }
            }
            Some(except) => {
                let keys: Vec<Key> = scene
                    .referents
                    .iter()
                    .filter(|r| fit(except, r).is_some())
                    .map(|r| r.key)
                    .collect();
                if keys.is_empty() {
                    return Resolution::Missing(format!(
                        "You can't see any {} here.",
                        phrase(except)
                    ));
                }
                keys
            }
            None => Vec::new(),
        };
        let mut all: Vec<&Referent> = scene
            .referents
            .iter()
            .filter(|r| domain.includes(r) && !except.contains(&r.key))
            .filter(|r| np.head.is_none() || fit(np, r).is_some())
            .collect();
        // What's within reach first, then the nearest, so one trip doesn't
        // walk away from the rest.
        all.sort_by_key(|r| (!r.reachable, r.position.map_or(0, distance)));
        let all: Vec<Key> = all.into_iter().map(|r| r.key).collect();
        return if all.is_empty() {
            Resolution::Missing(match domain {
                Domain::Carried => "You aren't carrying anything.".into(),
                _ => "There's nothing here to take.".into(),
            })
        } else {
            Resolution::Many(all)
        };
    }
    let fits: Vec<(&Referent, Fit)> = scene
        .referents
        .iter()
        .filter_map(|r| fit(np, r).map(|f| (r, f)))
        .collect();
    if fits.is_empty() {
        let words = phrase(np);
        return Resolution::Missing(if words.is_empty() {
            "You need to say what you mean.".into()
        } else if domain == Domain::Carried {
            format!("You aren't carrying any {words}.")
        } else {
            format!("You can't see any {words} here.")
        });
    }
    // What the verb can act on comes first, then how well the words fit:
    // "take scout" is the scout's corpse even though "scout" names the scout.
    let best_tier = fits
        .iter()
        .map(|(r, _)| domain.tier(r))
        .min()
        .expect("not empty");
    let fits: Vec<(&Referent, Fit)> = fits
        .into_iter()
        .filter(|(r, _)| domain.tier(r) == best_tier)
        .collect();
    // "Two arrows" counts from one stack; "arrows" means them all.
    let plural = np.quantity.is_none()
        && fits.iter().any(|(_, f)| *f == Fit::Plural)
        && !fits.iter().any(|(_, f)| *f == Fit::Head);
    let best_fit = fits.iter().map(|(_, f)| *f).max().expect("not empty");
    let candidates: Vec<&Referent> = fits
        .iter()
        .filter(|(_, f)| *f == best_fit)
        .map(|(r, _)| *r)
        .collect();
    if plural {
        return Resolution::Many(candidates.iter().map(|r| r.key).collect());
    }
    if let Some(ordinal) = np.ordinal {
        // "The second token" counts every token, alike or not.
        return match candidates.get(ordinal.wrapping_sub(1)) {
            Some(r) => Resolution::One(r.key),
            None => Resolution::Missing(format!(
                "You can't see that many {} here.",
                prose::plural(&phrase(np))
            )),
        };
    }
    // Indistinguishable things are interchangeable: one of a kind, chosen by
    // reach and distance.
    let mut kinds: BTreeMap<&str, Vec<&Referent>> = BTreeMap::new();
    for r in &candidates {
        kinds.entry(r.identity.as_str()).or_default().push(r);
    }
    let mut representatives: Vec<&Referent> = kinds
        .into_values()
        .map(|group| {
            *group
                .iter()
                .min_by_key(|r| (!r.reachable, r.position.map_or(0, distance)))
                .expect("not empty")
        })
        .collect();
    representatives.sort_by_key(|r| {
        scene
            .referents
            .iter()
            .position(|o| o.key == r.key)
            .unwrap_or(usize::MAX)
    });
    match representatives.as_slice() {
        [one] => Resolution::One(one.key),
        many => Resolution::Ask(many.iter().map(|r| r.key).collect()),
    }
}

fn pronoun_referents(pronoun: Pronoun, scene: &Scene, referents: &Referents) -> Resolution {
    let (word, keys): (&str, Vec<Key>) = match pronoun {
        Pronoun::It => ("it", referents.it.into_iter().collect()),
        Pronoun::Them => (
            "them",
            if referents.them.is_empty() {
                referents.it.into_iter().collect()
            } else {
                referents.them.clone()
            },
        ),
        Pronoun::Him => ("him", referents.him.into_iter().collect()),
        Pronoun::Her => ("her", referents.her.into_iter().collect()),
    };
    if keys.is_empty() {
        return Resolution::Missing(format!("I'm not sure what \"{word}\" refers to."));
    }
    let present: Vec<Key> = keys
        .iter()
        .copied()
        .filter(|k| scene.get(*k).is_some())
        .collect();
    match present.as_slice() {
        [] => Resolution::Missing(format!("You can't see {word} any more.")),
        [one] => Resolution::One(*one),
        _ => Resolution::Many(present),
    }
}
