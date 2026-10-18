//! Advanced natural language Interactive Fiction (IF) parser.
//!
//! Provides sentence tokenization, lexicon and grammar matching. Resolving
//! what the words refer to is the engine's job; see `crate::engine`.

pub mod grammar;
pub mod idiom;
pub mod lexicon;
pub mod noun_phrase;
pub mod token;

pub use grammar::{match_sentence, match_sentence_with_raw, ParsedCommand, SessionCommand};
pub use idiom::{Meta, Modifiers, Reading};
pub use lexicon::{Preposition, Pronoun, Verb};
pub use noun_phrase::{parse_noun_phrase, NounPhrase};
pub use token::{split_sentences, tokenize, Token};

/// Techniques the help names, so asking for one not granted says so rather
/// than not understanding.
const TECHNIQUES: &[&str] = &[
    "rattling blow",
    "measured cut",
    "brace for impact",
    "spark lance",
    "falter",
    "shell",
    "bind wounds",
];

/// An ability's name as words: "Spark Lance (Fire)" is spark, lance, fire.
fn name_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace(['(', ')', '_'], " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The granted ability the sentence's first words begin to name, when they
/// name only one: "brace" for Brace for Impact. Returns it with the rest of
/// the sentence, or why the words fit no one ability.
fn ability_prefix<'a>(
    view: &'a tor_protocol::Observation,
    raw: &str,
) -> Option<Result<(&'a tor_protocol::AbilityView, String), String>> {
    let words = name_words(raw);
    let first = words.first()?;
    if lexicon::parse_verb(first).is_some() || lexicon::parse_direction(first).is_some() {
        return None;
    }
    let mut found: Vec<(usize, &tor_protocol::AbilityView, Vec<String>)> = Vec::new();
    for ability in tor_client_common::abilities::available(view)
        .iter()
        .filter(|a| !a.basic_attack)
    {
        let alias = name_words(&ability.name);
        let k = alias.iter().zip(&words).take_while(|(a, w)| a == w).count();
        if k > 0 {
            found.push((k, ability, alias));
        }
    }
    let best = found.iter().map(|(k, ..)| *k).max()?;
    let found: Vec<_> = found.into_iter().filter(|(k, ..)| *k == best).collect();
    if let [_, _, ..] = found.as_slice() {
        let names: Vec<String> = found.iter().map(|(_, a, _)| a.name.clone()).collect();
        return Some(Err(format!(
            "Which do you mean, {}?",
            crate::engine::prose::or_list(&names)
        )));
    }
    let (k, ability, alias) = found.into_iter().next()?;
    let rest: Vec<String> = words[k..].to_vec();
    // A form not granted: "spark lance fire" when only the cold form is.
    if let (Some(next), Some(wanted)) = (rest.first(), alias.get(k)) {
        let names_someone = view
            .visible_actors
            .iter()
            .any(|a| name_words(&a.name).contains(next))
            || matches!(
                next.as_str(),
                "me" | "self" | "myself" | "it" | "him" | "her" | "them" | "the" | "a" | "an"
            );
        if next != wanted && !names_someone && ability.name.contains('(') {
            return Some(Err(format!("You know only {}.", ability.name)));
        }
    }
    Some(Ok((ability, rest.join(" "))))
}

/// Resolve content verbs against the current disclosure before the closed grammar.
pub fn match_disclosed_sentence(
    tokens: &[Token],
    raw: &str,
    view: &tor_protocol::Observation,
) -> Result<ParsedCommand, String> {
    let matched = match tor_client_common::abilities::match_command(view, raw)? {
        Some((ability, noun)) => Some((ability, noun.to_owned())),
        None => match ability_prefix(view, raw) {
            Some(Ok(found)) => Some(found),
            Some(Err(why)) => return Ok(ParsedCommand::Refused(why)),
            None => None,
        },
    };
    if let Some((ability, noun)) = matched {
        let (noun, supplies) = tor_client_common::abilities::split_supply_clause(&noun)?;
        if matches!(
            ability.target_kind,
            tor_protocol::AbilityTargetKind::SelfStep
                | tor_protocol::AbilityTargetKind::DirectionalRay
        ) {
            if noun.is_empty() {
                return Ok(ParsedCommand::Orphan {
                    prompt: format!("Which way do you want to use {}?", ability.name),
                    before: ability.name.to_lowercase(),
                    after: String::new(),
                });
            }
            let direction = lexicon::parse_direction(&noun.to_ascii_lowercase())
                .ok_or("Choose a direction for that ability.")?;
            return Ok(ParsedCommand::Ability {
                id: ability.id.clone(),
                selection: if ability.target_kind == tor_protocol::AbilityTargetKind::DirectionalRay
                {
                    grammar::AbilitySelection::DirectionalRay { direction }
                } else {
                    grammar::AbilitySelection::SelfStep { direction }
                },
                supplies: supplies.map(str::to_owned),
            });
        }
        let self_target = matches!(
            ability.target_kind,
            tor_protocol::AbilityTargetKind::SelfOnly | tor_protocol::AbilityTargetKind::SelfOrAlly
        );
        if noun.is_empty() && !self_target {
            return Ok(ParsedCommand::Orphan {
                prompt: format!("Who do you want to use {} on?", ability.name),
                before: ability.name.to_lowercase(),
                after: String::new(),
            });
        }
        let noun = if noun.is_empty() { "me" } else { noun };
        return Ok(ParsedCommand::Ability {
            id: ability.id.clone(),
            selection: grammar::AbilitySelection::ActorTarget {
                direct: parse_noun_phrase(&tokenize(noun))?,
            },
            supplies: supplies.map(str::to_owned),
        });
    }
    let words = name_words(raw).join(" ");
    if let Some(technique) = TECHNIQUES
        .iter()
        .find(|t| words == **t || words.starts_with(&format!("{t} ")))
    {
        return Ok(ParsedCommand::Refused(format!(
            "You haven't learned {}. stats lists the techniques you know.",
            crate::engine::prose::title(technique)
        )));
    }
    match idiom::read(raw) {
        Some(Reading::Meta(meta)) => Ok(ParsedCommand::Meta(meta)),
        Some(Reading::Command(text)) => match_sentence_with_raw(&tokenize(&text), Some(&text)),
        None => match_sentence_with_raw(tokens, Some(raw)),
    }
}

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
