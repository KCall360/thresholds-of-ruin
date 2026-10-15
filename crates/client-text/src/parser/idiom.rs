//! Idioms: the many ways players phrase a command, read as the one command
//! the grammar knows, and the words around a command that only colour it.
//!
//! [`normalize`] runs first: it expands contractions, drops possessive
//! endings, and strips politeness ("please", "I want to") and trailing
//! modifiers ("again", "carefully", "until it dies"), recording the ones that
//! change what the command means. [`read`] then matches the sentence against
//! a table of idioms, each a pattern over words:
//!
//! - a literal word, or alternatives `a|b`;
//! - an optional word in parentheses, `(on|the)`;
//! - `*`, one or more words that name something, copied into the reading.
//!
//! Of the patterns that match, the one with the most literal words wins, so
//! `keep attacking the scout` is a fight before it is anything else. New
//! phrasings are one more line in [`IDIOMS`].

use super::token::{tokenize, Token};

/// What the words around a command add to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    /// "again", "once more": do what was last done, to the same thing when
    /// nothing else is named.
    pub again: bool,
    /// "until it dies", "to the death": keep at it.
    pub persist: bool,
}

/// A question or remark about the game rather than an action in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meta {
    /// The ways out of here.
    Exits,
    /// Where the character is.
    Where,
    /// Who the character is.
    Who,
    /// What there is to do.
    WhatNow,
    Time,
    Undo,
    Restart,
    About,
    /// The client's settings.
    Options,
    Yes,
    No,
    /// An answer to something that has no effect in the game.
    Flavor(&'static str),
}

/// What an idiom reads as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reading {
    /// Command text the grammar understands.
    Command(String),
    Meta(Meta),
}

#[derive(Clone, Copy)]
enum Out {
    /// Command text; `*` stands for the words the slot matched.
    Command(&'static str),
    Meta(Meta),
}

use Meta::*;
use Out::{Command as C, Meta as M};

/// Phrasings and what they mean. Order breaks ties between equally long
/// matches.
const IDIOMS: &[(&str, Out)] = &[
    // Fighting on.
    (
        "keep (on) attacking|fighting|hitting|striking *",
        C("fight *"),
    ),
    ("keep (on) attacking|fighting|hitting|striking", C("fight")),
    ("continue|resume attacking|fighting *", C("fight *")),
    ("continue|resume attacking|fighting", C("fight")),
    ("fight *", C("fight *")),
    ("fight", C("fight")),
    ("finish (off) *", C("fight *")),
    ("finish * off", C("fight *")),
    // Other ways to say attack.
    (
        "swing|lunge|strike|hack|slash|thrust|cut|jab|aim at *",
        C("attack *"),
    ),
    ("swing (my|your) * at *", C("attack *")),
    (
        "charge|rush|engage|bash|cleave|whack|beat|clobber|batter|hew *",
        C("attack *"),
    ),
    ("go for *", C("attack *")),
    // Chasing.
    (
        "follow|chase|pursue|hunt|track|tail|shadow *",
        C("follow *"),
    ),
    ("go|run|head after *", C("follow *")),
    ("catch (up) (with|to) *", C("follow *")),
    // Getting away.
    ("run away", C("flee")),
    ("run off", C("flee")),
    ("flee|retreat|withdraw|escape", C("flee")),
    ("flee|retreat|escape|withdraw from *", C("flee")),
    ("run|get away from *", C("flee")),
    ("back off|away", C("flee")),
    ("fall back", C("flee")),
    // Going back.
    ("back", C("back")),
    ("go|head|turn|walk|step back", C("back")),
    ("go|head|turn|walk back the way (i|you) came", C("back")),
    ("return", C("back")),
    ("retrace (my|your) steps", C("back")),
    ("return to *", C("go to *")),
    (
        "head|walk|run|travel|make for|toward|towards *",
        C("go to *"),
    ),
    // Repeating.
    ("again", C("again")),
    ("repeat (that|it)", C("again")),
    ("do (it|that|this) again", C("again")),
    ("once more", C("again")),
    ("same again", C("again")),
    // Where to go.
    ("exits", M(Exits)),
    ("ways", M(Exits)),
    ("ways (out|on)", M(Exits)),
    ("which way|ways (can|could|should) i go", M(Exits)),
    ("which way", M(Exits)),
    ("where (can|could|should) i go", M(Exits)),
    ("what (are) (the) exits|ways (are) (there)", M(Exits)),
    ("list|show (the) exits", M(Exits)),
    ("is there a way out", M(Exits)),
    ("how do i get out", M(Exits)),
    // Where and who.
    ("where am i", M(Where)),
    ("where is this", M(Where)),
    ("what|which place is this", M(Where)),
    ("what is this place", M(Where)),
    ("where are we", M(Where)),
    ("who am i", M(Who)),
    ("what am i", M(Who)),
    ("who are you", M(Who)),
    ("whoami", M(Who)),
    // What to do.
    (
        "what (can|should|do|could|shall) i do (now|next|here)",
        M(WhatNow),
    ),
    ("what now", M(WhatNow)),
    ("what next", M(WhatNow)),
    ("now what", M(WhatNow)),
    ("what do i do", M(WhatNow)),
    ("what is my goal|objective|quest|task|mission", C("score")),
    ("what am i (supposed|meant) to do", M(WhatNow)),
    ("i am stuck", M(WhatNow)),
    ("stuck", M(WhatNow)),
    // Inventory and condition.
    (
        "what am i carrying|holding|wearing|wielding",
        C("inventory"),
    ),
    ("what do i have|carry|hold", C("inventory")),
    ("what have i got", C("inventory")),
    ("take|check|show|list (my) inventory", C("inventory")),
    (
        "check|search (my) pack|bag|pockets|belongings|gear|things",
        C("inventory"),
    ),
    ("how am i (doing|feeling)", C("diagnose")),
    ("how do i feel|look", C("diagnose")),
    ("am i hurt|wounded|injured|ok|okay|alright", C("diagnose")),
    (
        "check|examine (my) wounds|health|self|condition",
        C("diagnose"),
    ),
    ("hp|health|condition|wounds", C("diagnose")),
    // Asking about things.
    ("what is|was that|this|it", C("examine it")),
    ("what is|was *", C("examine *")),
    ("who is|was *", C("examine *")),
    ("what does * look like", C("examine *")),
    ("look (closely|carefully) at *", C("examine *")),
    ("look around", C("look")),
    // The game itself.
    ("time", M(Time)),
    ("what time is it", M(Time)),
    ("how long have i been here", M(Time)),
    ("undo|oops", M(Undo)),
    ("take (that|it) back", M(Undo)),
    ("restart|restore|reset", M(Restart)),
    ("new game", M(Restart)),
    ("start (over|again)", M(Restart)),
    ("play again", M(Restart)),
    ("about|credits|info|version", M(About)),
    ("options|settings|preferences|config", M(Options)),
    // Answers.
    ("yes|y|yeah|yep|yup|sure|ok|okay|aye|affirmative", M(Yes)),
    ("yes please", M(Yes)),
    ("do it", M(Yes)),
    ("go ahead", M(Yes)),
    ("no|nope|nah|negative", M(No)),
    ("no thanks", M(No)),
    ("never mind", M(No)),
    ("nevermind", M(No)),
    // Things that change nothing.
    (
        "xyzzy|plugh|plover|frotz",
        M(Flavor("A hollow voice says, \"Fool.\"")),
    ),
    (
        "dance",
        M(Flavor(
            "You shuffle through a few steps. The stones are unimpressed.",
        )),
    ),
    (
        "sing|hum|whistle",
        M(Flavor(
            "You hum a few bars under your breath. The stone swallows the sound.",
        )),
    ),
    (
        "think|ponder|consider|meditate|reflect",
        M(Flavor(
            "You think it over. Whatever you decide, you'll have to do it.",
        )),
    ),
    (
        "laugh|smile|grin|chuckle",
        M(Flavor("Your laugh sounds thin in the dark.")),
    ),
    (
        "cry|weep|sob|sigh|groan",
        M(Flavor("You allow yourself a moment. Then you go on.")),
    ),
    (
        "curse|swear|damn|dammit|shit|fuck|bother",
        M(Flavor("You mutter a curse. It helps, a little.")),
    ),
    (
        "scream|shout|yell|holler",
        M(Flavor(
            "Your shout rolls away into the dark. Nothing answers it.",
        )),
    ),
    ("wave", M(Flavor("You wave. No one waves back."))),
    (
        "hello|hi|hey|greetings",
        M(Flavor("Your greeting echoes away unanswered.")),
    ),
    (
        "thanks|thank you",
        M(Flavor("You're welcome, though no one here said so.")),
    ),
    ("kiss *", M(Flavor("This is hardly the time or the place."))),
    ("hug *", M(Flavor("This is hardly the time or the place."))),
];

/// Words that begin sentences whose text must be kept as typed.
const VERBATIM: &[&str] = &[
    "say",
    "note",
    "bookmark",
    "name",
    "annotate",
    "wizard",
    "history",
    "branch-history",
    "pace",
    "all",
];

/// Words ahead of a command that ask for it politely or say it's wanted.
const OPENERS: &[&[&str]] = &[
    &["please"],
    &["now"],
    &["ok"],
    &["okay"],
    &["so"],
    &["i", "want", "to"],
    &["i", "wanna"],
    &["i", "would", "like", "to"],
    &["i", "will"],
    &["i", "shall"],
    &["i", "should"],
    &["i", "try", "to"],
    &["let", "us"],
    &["let", "me"],
    &["try", "to"],
    &["try", "and"],
    &["attempt", "to"],
    &["can", "i"],
    &["could", "i"],
    &["may", "i"],
    &["can", "you"],
    &["could", "you"],
    &["would", "you"],
    &["will", "you"],
    &["go", "and"],
    &["carefully"],
    &["quickly"],
    &["quietly"],
    &["slowly"],
];

/// Words after a command that only say how it's done.
const MANNERS: &[&str] = &[
    "please",
    "now",
    "carefully",
    "quickly",
    "slowly",
    "quietly",
    "gently",
    "firmly",
    "hard",
    "harder",
    "fiercely",
    "boldly",
    "cautiously",
    "immediately",
    "anyway",
    "too",
];

/// Contractions, as their words.
fn expand(word: &str) -> Option<&'static [&'static str]> {
    Some(match word {
        "what's" => &["what", "is"],
        "where's" => &["where", "is"],
        "who's" => &["who", "is"],
        "that's" => &["that", "is"],
        "it's" => &["it", "is"],
        "there's" => &["there", "is"],
        "how's" => &["how", "is"],
        "i'm" => &["i", "am"],
        "i'd" => &["i", "would"],
        "i'll" => &["i", "will"],
        "i've" => &["i", "have"],
        "let's" => &["let", "us"],
        "can't" | "cannot" => &["can", "not"],
        "don't" => &["do", "not"],
        "won't" => &["will", "not"],
        "isn't" => &["is", "not"],
        _ => return None,
    })
}

/// The sentence with contractions expanded, possessives and politeness
/// dropped, and its modifiers set aside. Sentences that keep their text as
/// typed (notes, names, quotations) come back unchanged.
pub fn normalize(sentence: &str) -> (String, Modifiers) {
    let tokens = tokenize(sentence);
    let first = tokens.first().and_then(Token::as_word).unwrap_or("");
    if tokens.iter().any(|t| matches!(t, Token::Quoted(_))) || VERBATIM.contains(&first) {
        return (sentence.trim().to_owned(), Modifiers::default());
    }
    let mut words: Vec<String> = Vec::new();
    for token in &tokens {
        match token {
            Token::Word(w) => match expand(w) {
                Some(parts) => words.extend(parts.iter().map(|p| (*p).to_owned())),
                // "the scout's body" names the body by the scout.
                None => words.push(w.strip_suffix("'s").unwrap_or(w).replace('\'', "")),
            },
            other => words.push(other.text()),
        }
    }
    let mut modifiers = Modifiers::default();
    // Openers, as many as there are, leaving at least one word.
    loop {
        let found = OPENERS
            .iter()
            .find(|o| words.len() > o.len() && words.iter().zip(o.iter()).all(|(w, o)| w == o));
        match found {
            Some(o) => {
                words.drain(..o.len());
            }
            None => break,
        }
    }
    // A whole idiom keeps its words: "what now" is not "what".
    if read(&words.join(" ")).is_some() {
        return (words.join(" "), modifiers);
    }
    // Trailing modifiers, in any order.
    loop {
        let n = words.len();
        let ends = |tail: &[&str]| {
            n > tail.len()
                && words[n - tail.len()..]
                    .iter()
                    .zip(tail)
                    .all(|(w, t)| w == t)
        };
        if n > 1 && words[n - 1] == "," {
            words.pop();
        } else if ends(&["again"]) {
            modifiers.again = true;
            words.pop();
        } else if ends(&["once", "more"]) || ends(&["one", "more", "time"]) {
            modifiers.again = true;
            words.truncate(n - if ends(&["once", "more"]) { 2 } else { 3 });
        } else if ends(&["at", "once"]) || ends(&["right", "now"]) || ends(&["for", "me"]) {
            words.truncate(n - 2);
        } else if ends(&["to", "the", "death"]) {
            modifiers.persist = true;
            words.truncate(n - 3);
        } else if ends(&["to", "death"]) {
            modifiers.persist = true;
            words.truncate(n - 2);
        } else if let Some(at) = until(&words) {
            modifiers.persist = true;
            words.truncate(at);
        } else if n > 1 && MANNERS.contains(&words[n - 1].as_str()) {
            words.pop();
        } else {
            break;
        }
    }
    if words.is_empty() {
        return (sentence.trim().to_owned(), modifiers);
    }
    (words.join(" "), modifiers)
}

/// Where "until it dies" begins, when the sentence ends with one.
fn until(words: &[String]) -> Option<usize> {
    let at = words
        .iter()
        .rposition(|w| matches!(w.as_str(), "until" | "till" | "til"))?;
    let tail = &words[at + 1..];
    let ending = [
        "it",
        "he",
        "she",
        "they",
        "them",
        "is",
        "are",
        "dead",
        "dies",
        "die",
        "falls",
        "fall",
        "drops",
        "drop",
        "stops",
        "stop",
        "moving",
        "down",
        "destroyed",
        "gone",
        "over",
        "done",
        "finished",
        "the",
        "fight",
        "ends",
    ];
    (at > 0 && !tail.is_empty() && tail.iter().all(|w| ending.contains(&w.as_str()))).then_some(at)
}

#[derive(Clone, Debug)]
enum Part {
    Word(Vec<&'static str>),
    Optional(Vec<&'static str>),
    Slot,
}

fn parts(pattern: &'static str) -> Vec<Part> {
    pattern
        .split_whitespace()
        .map(|p| {
            if p == "*" {
                Part::Slot
            } else if let Some(inner) = p.strip_prefix('(').and_then(|p| p.strip_suffix(')')) {
                Part::Optional(inner.split('|').collect())
            } else {
                Part::Word(p.split('|').collect())
            }
        })
        .collect()
}

/// Words the pattern's literals matched, and the words in its slots, if it
/// matches every word.
fn matches(parts: &[Part], words: &[&str]) -> Option<(usize, Vec<String>)> {
    match parts.split_first() {
        None => words.is_empty().then(|| (0, Vec::new())),
        Some((Part::Word(alternatives), rest)) => {
            let (first, others) = words.split_first()?;
            if !alternatives.contains(first) {
                return None;
            }
            matches(rest, others).map(|(n, slots)| (n + 1, slots))
        }
        Some((Part::Optional(alternatives), rest)) => {
            let taken = words
                .split_first()
                .filter(|(first, _)| alternatives.contains(first))
                .and_then(|(_, others)| matches(rest, others))
                .map(|(n, slots)| (n + 1, slots));
            taken.or_else(|| matches(rest, words))
        }
        Some((Part::Slot, rest)) => {
            // The shortest slot that lets the rest match.
            (1..=words.len()).find_map(|end| {
                matches(rest, &words[end..]).map(|(n, mut slots)| {
                    slots.insert(0, words[..end].join(" "));
                    (n, slots)
                })
            })
        }
    }
}

/// What an idiom makes of the sentence, if any matches.
pub fn read(sentence: &str) -> Option<Reading> {
    let tokens = tokenize(sentence);
    if tokens.iter().any(|t| !matches!(t, Token::Word(_))) {
        return None;
    }
    let words: Vec<&str> = tokens.iter().filter_map(Token::as_word).collect();
    let mut best: Option<(usize, Out, Vec<String>)> = None;
    for (pattern, out) in IDIOMS {
        if let Some((score, slots)) = matches(&parts(pattern), &words) {
            if best.as_ref().is_none_or(|(s, ..)| score > *s) {
                best = Some((score, *out, slots));
            }
        }
    }
    let (_, out, slots) = best?;
    Some(match out {
        Out::Meta(meta) => Reading::Meta(meta),
        Out::Command(text) => {
            // The last slot is what the command acts on.
            let mut text = text.to_owned();
            if let Some(slot) = slots.last() {
                text = text.replacen('*', slot, 1);
            }
            Reading::Command(text)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(text: &str) -> Option<String> {
        match read(text)? {
            Reading::Command(c) => Some(c),
            Reading::Meta(_) => None,
        }
    }

    #[test]
    fn modifiers_are_set_aside() {
        assert_eq!(
            normalize("attack again"),
            (
                "attack".into(),
                Modifiers {
                    again: true,
                    persist: false
                }
            )
        );
        assert_eq!(
            normalize("attack the scout until it dies"),
            (
                "attack the scout".into(),
                Modifiers {
                    again: false,
                    persist: true
                }
            )
        );
        assert_eq!(
            normalize("examine the scout carefully").0,
            "examine the scout"
        );
        assert_eq!(normalize("please take the sword").0, "take the sword");
        assert_eq!(
            normalize("I want to open the door, please").0,
            "open the door"
        );
        assert!(normalize("hit it once more").1.again);
        assert!(normalize("attack scout to the death").1.persist);
    }

    #[test]
    fn contractions_and_possessives_become_plain_words() {
        assert_eq!(normalize("what's that").0, "what is that");
        assert_eq!(normalize("take the scout's body").0, "take the scout body");
        assert_eq!(normalize("I'd like to go north").0, "go north");
    }

    #[test]
    fn verbatim_sentences_keep_their_text() {
        assert_eq!(
            normalize("note Don't trust the Guardian again").0,
            "note Don't trust the Guardian again"
        );
        assert_eq!(normalize("say \"hello again\"").0, "say \"hello again\"");
    }

    #[test]
    fn a_lone_modifier_is_kept() {
        assert_eq!(normalize("again").0, "again");
        assert_eq!(normalize("now").0, "now");
        // Regression: "what now" lost its "now" and read as "what".
        assert_eq!(normalize("what now").0, "what now");
        assert_eq!(normalize("once more").0, "once more");
    }

    #[test]
    fn idioms_read_as_commands() {
        assert_eq!(
            command("keep attacking the scout").as_deref(),
            Some("fight the scout")
        );
        assert_eq!(command("keep attacking").as_deref(), Some("fight"));
        assert_eq!(
            command("swing at the scout").as_deref(),
            Some("attack the scout")
        );
        assert_eq!(command("charge scout").as_deref(), Some("attack scout"));
        assert_eq!(
            command("chase the wisp").as_deref(),
            Some("follow the wisp")
        );
        assert_eq!(
            command("go after the wisp").as_deref(),
            Some("follow the wisp")
        );
        assert_eq!(command("run away").as_deref(), Some("flee"));
        assert_eq!(command("go back").as_deref(), Some("back"));
        assert_eq!(
            command("return to the threshold").as_deref(),
            Some("go to the threshold")
        );
        assert_eq!(command("what am i carrying").as_deref(), Some("inventory"));
        assert_eq!(command("what is that").as_deref(), Some("examine it"));
        assert_eq!(
            command("what is the dawn seal").as_deref(),
            Some("examine the dawn seal")
        );
        assert_eq!(
            command("finish off the guardian").as_deref(),
            Some("fight the guardian")
        );
        assert_eq!(command("do it again").as_deref(), Some("again"));
        assert_eq!(
            command("swing my sword at the scout").as_deref(),
            Some("attack the scout")
        );
    }

    #[test]
    fn questions_read_as_meta() {
        assert_eq!(read("where am i"), Some(Reading::Meta(Meta::Where)));
        assert_eq!(read("exits"), Some(Reading::Meta(Meta::Exits)));
        assert_eq!(read("where can i go"), Some(Reading::Meta(Meta::Exits)));
        assert_eq!(read("what can i do"), Some(Reading::Meta(Meta::WhatNow)));
        assert_eq!(read("who am i"), Some(Reading::Meta(Meta::Who)));
        assert_eq!(read("undo"), Some(Reading::Meta(Meta::Undo)));
        assert_eq!(read("yes"), Some(Reading::Meta(Meta::Yes)));
        assert!(matches!(
            read("xyzzy"),
            Some(Reading::Meta(Meta::Flavor(_)))
        ));
    }

    #[test]
    fn ordinary_commands_are_left_to_the_grammar() {
        for text in [
            "take the sword",
            "north",
            "n",
            "attack scout",
            "go east",
            "run east",
        ] {
            assert_eq!(read(text), None, "{text}");
        }
    }
}
