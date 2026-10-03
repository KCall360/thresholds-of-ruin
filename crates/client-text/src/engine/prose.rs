//! English realization: articles, plurals, numbers, lists and sentences.

/// Words that start with a vowel letter but a consonant sound, and the
/// reverse. Prefixes are checked against the start of a lowercase word.
const CONSONANT_SOUNDS: &[&str] = &[
    "one", "once", "uni", "use", "usu", "uti", "ure", "uro", "eu", "ewe",
];
const VOWEL_SOUNDS: &[&str] = &["hour", "honest", "honou", "heir"];

/// "a" or "an" by how the phrase is pronounced.
pub fn article(phrase: &str) -> &'static str {
    let word = phrase
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();
    if VOWEL_SOUNDS.iter().any(|p| word.starts_with(p)) {
        return "an";
    }
    if CONSONANT_SOUNDS.iter().any(|p| word.starts_with(p)) {
        return "a";
    }
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

/// "an echoing hall".
pub fn indefinite(phrase: &str) -> String {
    format!("{} {phrase}", article(phrase))
}

/// "the ruin scout".
pub fn definite(phrase: &str) -> String {
    format!("the {phrase}")
}

/// The first letter in upper case.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A sentence: capitalized, ending in a full stop unless it already ends in
/// punctuation.
pub fn sentence(text: &str) -> String {
    let text = capitalize(text.trim());
    if text.is_empty() || text.ends_with(['.', '!', '?']) {
        text
    } else {
        format!("{text}.")
    }
}

/// "a", "a and b", "a, b and c".
pub fn list(items: &[String], conjunction: &str) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} {conjunction} {last}", rest.join(", ")),
    }
}

pub fn and_list(items: &[String]) -> String {
    list(items, "and")
}

pub fn or_list(items: &[String]) -> String {
    list(items, "or")
}

/// Small counts as words, larger ones as numerals.
pub fn number(n: u64) -> String {
    const WORDS: [&str; 13] = [
        "no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    WORDS
        .get(n as usize)
        .map_or_else(|| n.to_string(), |w| (*w).to_owned())
}

const IRREGULAR: &[(&str, &str)] = &[
    ("man", "men"),
    ("woman", "women"),
    ("child", "children"),
    ("foot", "feet"),
    ("tooth", "teeth"),
    ("mouse", "mice"),
    ("louse", "lice"),
    ("goose", "geese"),
    ("ox", "oxen"),
    ("staff", "staves"),
    ("dwarf", "dwarves"),
    ("elf", "elves"),
    ("wolf", "wolves"),
    ("knife", "knives"),
    ("life", "lives"),
    ("leaf", "leaves"),
    ("thief", "thieves"),
    ("sheep", "sheep"),
    ("fish", "fish"),
    ("deer", "deer"),
    ("armor", "armor"),
    ("armour", "armour"),
    ("mail", "mail"),
];

/// The plural of a noun phrase: its last word is pluralized.
pub fn plural(phrase: &str) -> String {
    let (head, last) = match phrase.rsplit_once(' ') {
        Some((head, last)) => (format!("{head} "), last),
        None => (String::new(), phrase),
    };
    format!("{head}{}", plural_word(last))
}

fn plural_word(word: &str) -> String {
    let lower = word.to_lowercase();
    if let Some((_, plural)) = IRREGULAR.iter().find(|(s, _)| *s == lower) {
        return (*plural).to_owned();
    }
    let ends = |suffix: &str| lower.ends_with(suffix);
    if ends("s") || ends("x") || ends("z") || ends("ch") || ends("sh") {
        format!("{word}es")
    } else if ends("y") && !lower[..lower.len() - 1].ends_with(['a', 'e', 'i', 'o', 'u']) {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

/// Whether `word` is the plural of `singular`, as [`plural`] forms it.
pub fn is_plural_of(word: &str, singular: &str) -> bool {
    word != singular && plural_word(singular) == word
}

/// "a copper token", "two copper tokens".
pub fn counted(count: u64, singular: &str) -> String {
    if count == 1 {
        indefinite(singular)
    } else {
        format!("{} {}", number(count), plural(singular))
    }
}

/// "the copper token", "the two copper tokens".
pub fn counted_definite(count: u64, singular: &str) -> String {
    if count == 1 {
        definite(singular)
    } else {
        format!("the {} {}", number(count), plural(singular))
    }
}

/// Passage text from sentences: one paragraph, single-spaced.
pub fn paragraph(sentences: &[String]) -> String {
    sentences
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn articles_follow_pronunciation() {
        assert_eq!(indefinite("echoing hall"), "an echoing hall");
        assert_eq!(indefinite("ruin scout"), "a ruin scout");
        assert_eq!(indefinite("unicorn"), "a unicorn");
        assert_eq!(indefinite("hour glass"), "an hour glass");
        assert_eq!(indefinite("open wooden door"), "an open wooden door");
        assert_eq!(indefinite("one-eyed rat"), "a one-eyed rat");
    }

    #[test]
    fn plurals_and_counts() {
        assert_eq!(plural("copper token"), "copper tokens");
        assert_eq!(plural("ruin scout corpse"), "ruin scout corpses");
        assert_eq!(plural("box"), "boxes");
        assert_eq!(plural("ruby"), "rubies");
        assert_eq!(plural("key"), "keys");
        assert_eq!(plural("throwing knife"), "throwing knives");
        assert_eq!(counted(1, "arrow"), "an arrow");
        assert_eq!(counted(3, "arrow"), "three arrows");
        assert_eq!(counted(40, "arrow"), "40 arrows");
        assert!(is_plural_of("tokens", "token"));
        assert!(!is_plural_of("token", "token"));
    }

    #[test]
    fn lists_and_sentences() {
        let words = |w: &[&str]| w.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            or_list(&words(&["north", "east", "west"])),
            "north, east or west"
        );
        assert_eq!(
            and_list(&words(&["a token", "a tablet"])),
            "a token and a tablet"
        );
        assert_eq!(and_list(&words(&["a token"])), "a token");
        assert_eq!(sentence("the ruin scout falls"), "The ruin scout falls.");
        assert_eq!(sentence("Victory!"), "Victory!");
    }
}
