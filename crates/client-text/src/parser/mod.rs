//! Advanced natural language Interactive Fiction (IF) parser.
//!
//! Provides sentence tokenization, lexicon and grammar matching. Resolving
//! what the words refer to is the engine's job; see `crate::engine`.

pub mod grammar;
pub mod lexicon;
pub mod noun_phrase;
pub mod token;

pub use grammar::{match_sentence, match_sentence_with_raw, ParsedCommand, SessionCommand};
pub use lexicon::{Preposition, Pronoun, Verb};
pub use noun_phrase::{parse_noun_phrase, NounPhrase};
pub use token::{split_sentences, tokenize, Token};

/// Parses a raw line of player input into one or more sequential ParsedCommands.
/// Supports multi-command sentences separated by '.', ';', or 'then'.
pub fn parse_input(line: &str) -> Result<Vec<ParsedCommand>, String> {
    let tokens = tokenize(line);
    let sentences = split_sentences(&tokens);

    if sentences.is_empty() {
        return Ok(Vec::new());
    }

    let mut commands = Vec::new();
    for sentence in sentences {
        commands.push(match_sentence(&sentence)?);
    }

    Ok(commands)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::Direction;

    #[test]
    fn parse_multi_command_line() {
        let commands = parse_input("take the copper token. go east. open the wooden door").unwrap();
        assert_eq!(commands.len(), 3);

        match &commands[0] {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(*verb, Verb::Take);
                assert_eq!(direct.head.as_deref(), Some("token"));
                assert_eq!(direct.adjectives, vec!["copper"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }

        match &commands[1] {
            ParsedCommand::Directional { direction } => {
                assert_eq!(*direction, Direction::East);
            }
            other => panic!("Unexpected: {other:?}"),
        }

        match &commands[2] {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(*verb, Verb::Open);
                assert_eq!(direct.head.as_deref(), Some("door"));
                assert_eq!(direct.adjectives, vec!["wooden"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn parse_ditransitive_attack_with() {
        let commands = parse_input("attack the goblin with the iron sword").unwrap();
        assert_eq!(commands.len(), 1);

        match &commands[0] {
            ParsedCommand::Ditransitive {
                verb,
                direct,
                preposition,
                indirect,
            } => {
                assert_eq!(*verb, Verb::Attack);
                assert_eq!(direct.head.as_deref(), Some("goblin"));
                assert_eq!(*preposition, Preposition::With);
                assert_eq!(indirect.head.as_deref(), Some("sword"));
                assert_eq!(indirect.adjectives, vec!["iron"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }
}
