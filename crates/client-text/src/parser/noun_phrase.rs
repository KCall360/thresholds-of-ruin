//! Noun phrase representation and parsing.

use super::{
    lexicon::{is_all, is_determiner, is_except, parse_ordinal, parse_pronoun, Pronoun},
    token::Token,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NounPhrase {
    pub raw: String,
    pub head: Option<String>,
    pub adjectives: Vec<String>,
    pub determiner: Option<String>,
    pub quantity: Option<u64>,
    pub ordinal: Option<usize>,
    pub pronoun: Option<Pronoun>,
    pub all: bool,
    pub is_one: bool,
    pub except: Option<Box<NounPhrase>>,
}

impl NounPhrase {
    pub fn empty() -> Self {
        Self {
            raw: String::new(),
            head: None,
            adjectives: Vec::new(),
            determiner: None,
            quantity: None,
            ordinal: None,
            pronoun: None,
            all: false,
            is_one: false,
            except: None,
        }
    }

    /// True if the noun phrase specifies a pronoun like "it" or "him".
    pub fn is_pronoun(&self) -> bool {
        self.pronoun.is_some()
    }
}

/// Splits a token slice containing conjunctions into multiple noun phrase token slices.
/// Handles:
/// - "sword and shield" -> ["sword"], ["shield"]
/// - "token, tablet, and lamp" -> ["token"], ["tablet"], ["lamp"]
pub fn split_conjunction_phrases(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut result = Vec::new();
    let mut current = Vec::new();
    let len = tokens.len();
    let mut i = 0;

    while i < len {
        let token = &tokens[i];

        // Check for comma
        if let Token::Punctuation(',') = token {
            if !current.is_empty() {
                result.push(std::mem::take(&mut current));
            }
            // Check if followed by "and"
            if i + 1 < len && tokens[i + 1].as_word() == Some("and") {
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        // Check for "and" (unless part of "and then", which should have been split earlier)
        if let Some(w) = token.as_word() {
            if w == "and" {
                if !current.is_empty() {
                    result.push(std::mem::take(&mut current));
                }
                i += 1;
                continue;
            }
        }

        current.push(token.clone());
        i += 1;
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Parses a token slice into a structured NounPhrase.
pub fn parse_noun_phrase(tokens: &[Token]) -> Result<NounPhrase, String> {
    if tokens.is_empty() {
        return Err("Missing noun phrase.".into());
    }

    let raw: String = tokens
        .iter()
        .map(|t| t.text())
        .collect::<Vec<_>>()
        .join(" ");

    let mut np = NounPhrase::empty();
    np.raw = raw;

    let len = tokens.len();
    let mut i = 0;

    // Check for "all" or "everything"
    if let Some(word) = tokens[i].as_word() {
        if is_all(word) {
            np.all = true;
            i += 1;

            // Check for "except <phrase>" or "but <phrase>"
            if i < len {
                if let Some(next_word) = tokens[i].as_word() {
                    if is_except(next_word) {
                        let except_phrase = parse_noun_phrase(&tokens[i + 1..])?;
                        np.except = Some(Box::new(except_phrase));
                        return Ok(np);
                    }
                }
            }
            if i == len {
                return Ok(np);
            }
        }
    }

    // Check for leading determiner: the, a, an, some, this, that
    if i < len {
        if let Some(word) = tokens[i].as_word() {
            if is_determiner(word) {
                np.determiner = Some(word.to_string());
                i += 1;
            }
        }
    }

    // Check for quantity (number) or ordinal
    if i < len {
        match &tokens[i] {
            Token::Number(n) => {
                np.quantity = Some(*n);
                i += 1;
            }
            Token::Word(w) => {
                if let Some(ord) = parse_ordinal(w) {
                    np.ordinal = Some(ord);
                    i += 1;
                }
            }
            _ => {}
        }
    }

    // Check for pronoun
    if i < len {
        if let Some(word) = tokens[i].as_word() {
            if let Some(pronoun) = parse_pronoun(word) {
                np.pronoun = Some(pronoun);
                i += 1;
                if i == len {
                    return Ok(np);
                }
            }
        }
    }

    // Remaining words are adjectives and head noun
    let mut words = Vec::new();
    while i < len {
        if let Some(word) = tokens[i].as_word() {
            words.push(word.to_string());
        }
        i += 1;
    }

    if words.is_empty() {
        if np.pronoun.is_some() || np.all || np.ordinal.is_some() {
            return Ok(np);
        }
        return Err("Missing noun.".into());
    }

    // Check if the last word is "one" (e.g. "copper one" or "the first one")
    if words.last().map(|s| s.as_str()) == Some("one") {
        np.is_one = true;
        words.pop();
        np.adjectives = words;
        return Ok(np);
    }

    // The last word is the head noun; prior words are adjectives
    np.head = words.pop();
    np.adjectives = words;

    Ok(np)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::token::tokenize;

    #[test]
    fn test_parse_simple_noun() {
        let tokens = tokenize("the copper token");
        let np = parse_noun_phrase(&tokens).unwrap();
        assert_eq!(np.determiner.as_deref(), Some("the"));
        assert_eq!(np.adjectives, vec!["copper"]);
        assert_eq!(np.head.as_deref(), Some("token"));
        assert_eq!(np.quantity, None);
    }

    #[test]
    fn test_parse_quantity_noun() {
        let tokens = tokenize("3 arrows");
        let np = parse_noun_phrase(&tokens).unwrap();
        assert_eq!(np.quantity, Some(3));
        assert_eq!(np.head.as_deref(), Some("arrows"));
    }

    #[test]
    fn test_parse_ordinal() {
        let tokens = tokenize("the second door");
        let np = parse_noun_phrase(&tokens).unwrap();
        assert_eq!(np.ordinal, Some(2));
        assert_eq!(np.head.as_deref(), Some("door"));
    }

    #[test]
    fn test_parse_the_copper_one() {
        let tokens = tokenize("the copper one");
        let np = parse_noun_phrase(&tokens).unwrap();
        assert_eq!(np.determiner.as_deref(), Some("the"));
        assert_eq!(np.adjectives, vec!["copper"]);
        assert_eq!(np.head.as_deref(), None);
        assert!(np.is_one);
    }

    #[test]
    fn test_parse_pronoun() {
        let tokens = tokenize("it");
        let np = parse_noun_phrase(&tokens).unwrap();
        assert_eq!(np.pronoun, Some(Pronoun::It));
    }

    #[test]
    fn test_parse_all_except() {
        let tokens = tokenize("all except the copper token");
        let np = parse_noun_phrase(&tokens).unwrap();
        assert!(np.all);
        let except = np.except.unwrap();
        assert_eq!(except.adjectives, vec!["copper"]);
        assert_eq!(except.head.as_deref(), Some("token"));
    }

    #[test]
    fn test_split_conjunction() {
        let tokens = tokenize("copper token, silver token, and stone tablet");
        let parts = split_conjunction_phrases(&tokens);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], tokenize("copper token"));
        assert_eq!(parts[1], tokenize("silver token"));
        assert_eq!(parts[2], tokenize("stone tablet"));
    }
}
