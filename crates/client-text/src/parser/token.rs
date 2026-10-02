//! Tokenization and sentence splitting for Interactive Fiction input.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    Word(String),
    Number(u64),
    Punctuation(char),
    Quoted(String),
}

impl Token {
    pub fn as_word(&self) -> Option<&str> {
        match self {
            Token::Word(w) => Some(w.as_str()),
            _ => None,
        }
    }

    pub fn text(&self) -> String {
        match self {
            Token::Word(w) => w.clone(),
            Token::Number(n) => n.to_string(),
            Token::Punctuation(c) => c.to_string(),
            Token::Quoted(q) => format!("\"{q}\""),
        }
    }
}

/// Tokenizes a raw line of input into tokens.
/// Normalizes words to lowercase while preserving string literals.
pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Quoted literals: "..." or '...'
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let start = i;
            while i < len && chars[i] != quote {
                i += 1;
            }
            let literal: String = chars[start..i].iter().collect();
            tokens.push(Token::Quoted(literal));
            if i < len && chars[i] == quote {
                i += 1;
            }
            continue;
        }

        // Punctuation characters that act as sentence or clause delimiters
        if matches!(c, '.' | ';' | '!' | '?' | ',') {
            tokens.push(Token::Punctuation(c));
            i += 1;
            continue;
        }

        // Numbers
        if c.is_ascii_digit() {
            let start = i;
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            let num_str: String = chars[start..i].iter().collect();
            if let Ok(num) = num_str.parse::<u64>() {
                tokens.push(Token::Number(num));
            } else {
                tokens.push(Token::Word(num_str));
            }
            continue;
        }

        // Words (alphanumeric plus hyphens or apostrophes within words)
        let start = i;
        while i < len
            && !chars[i].is_whitespace()
            && !matches!(chars[i], '.' | ';' | '!' | '?' | ',' | '"' | '\'')
        {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        tokens.push(Token::Word(word.to_lowercase()));
    }

    tokens
}

/// Splits a token sequence into individual sentence chunks.
/// Delimiters include '.', ';', '!', '?', or the word 'then'.
/// Also handles 'and then'.
pub fn split_sentences(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut sentences = Vec::new();
    let mut current = Vec::new();
    let len = tokens.len();
    let mut i = 0;

    while i < len {
        let token = &tokens[i];

        // Check for sentence-ending punctuation: . ; ! ?
        if let Token::Punctuation(p) = token {
            if matches!(p, '.' | ';' | '!' | '?') {
                if !current.is_empty() {
                    sentences.push(std::mem::take(&mut current));
                }
                i += 1;
                continue;
            }
        }

        // Check for "and then" or "then"
        if let Some(word) = token.as_word() {
            if word == "then" {
                if !current.is_empty() {
                    sentences.push(std::mem::take(&mut current));
                }
                i += 1;
                continue;
            } else if word == "and" && i + 1 < len {
                if let Some(next_word) = tokens[i + 1].as_word() {
                    if next_word == "then" {
                        if !current.is_empty() {
                            sentences.push(std::mem::take(&mut current));
                        }
                        i += 2;
                        continue;
                    }
                }
            }
        }

        current.push(token.clone());
        i += 1;
    }

    if !current.is_empty() {
        sentences.push(current);
    }

    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_basic() {
        let tokens = tokenize("take the copper token");
        assert_eq!(
            tokens,
            vec![
                Token::Word("take".into()),
                Token::Word("the".into()),
                Token::Word("copper".into()),
                Token::Word("token".into()),
            ]
        );
    }

    #[test]
    fn tokenize_punctuation_and_numbers() {
        let tokens = tokenize("take 3 arrows; then go north.");
        assert_eq!(
            tokens,
            vec![
                Token::Word("take".into()),
                Token::Number(3),
                Token::Word("arrows".into()),
                Token::Punctuation(';'),
                Token::Word("then".into()),
                Token::Word("go".into()),
                Token::Word("north".into()),
                Token::Punctuation('.'),
            ]
        );
    }

    #[test]
    fn periods_split_sentences() {
        let tokens = tokenize("take lamp. go east. open door");
        let sentences = split_sentences(&tokens);
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0], tokenize("take lamp"));
        assert_eq!(sentences[1], tokenize("go east"));
        assert_eq!(sentences[2], tokenize("open door"));
    }

    #[test]
    fn split_sentences_then_and_then() {
        let tokens = tokenize("take lamp and then go east then look");
        let sentences = split_sentences(&tokens);
        assert_eq!(sentences.len(), 3);
        assert_eq!(sentences[0], tokenize("take lamp"));
        assert_eq!(sentences[1], tokenize("go east"));
        assert_eq!(sentences[2], tokenize("look"));
    }
}
