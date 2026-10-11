//! Sentence-level grammar and syntax pattern matching.

use tor_protocol::Direction;

use super::{
    lexicon::{parse_direction, parse_preposition, parse_verb, Preposition, Verb},
    noun_phrase::{parse_noun_phrase, split_conjunction_phrases, NounPhrase},
    token::Token,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionCommand {
    Save,
    /// Show or set the milliseconds between shown journey steps.
    Pace(Option<u64>),
    Sync,
    Control,
    Release,
    History {
        before: Option<String>,
    },
    BranchHistory {
        branch: String,
        before: Option<String>,
    },
    Places,
    Note {
        text: String,
        is_bookmark: bool,
    },
    Name {
        target: String,
        name: String,
    },
    Annotate {
        source: String,
        audience: String,
        category: String,
        anchor: String,
        text: String,
    },
    Wizard {
        command: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedCommand {
    Intransitive {
        verb: Verb,
    },
    Directional {
        direction: Direction,
    },
    Transitive {
        verb: Verb,
        direct: NounPhrase,
    },
    Ditransitive {
        verb: Verb,
        direct: NounPhrase,
        preposition: Preposition,
        indirect: NounPhrase,
    },
    MultiTransitive {
        verb: Verb,
        direct_list: Vec<NounPhrase>,
    },
    Step {
        direction: Direction,
    },
    Say {
        text: String,
    },
    Stop,
    Again,
    Help {
        topic: Option<String>,
    },
    /// A conversational follow-up (e.g. "the copper one", "copper", "first")
    /// answering a previous disambiguation question.
    Clarification(NounPhrase),
    Session(SessionCommand),
}

/// Matches a sentence of tokens to a ParsedCommand.
pub fn match_sentence(tokens: &[Token]) -> Result<ParsedCommand, String> {
    match_sentence_with_raw(tokens, None)
}

/// Matches a sentence of tokens with optional raw string to preserve literal text.
pub fn match_sentence_with_raw(
    tokens: &[Token],
    raw_line: Option<&str>,
) -> Result<ParsedCommand, String> {
    if tokens.is_empty() {
        return Err("Please say what you want to do.".into());
    }

    // 1. Check for single direction (e.g. "north", "ne", "up")
    if tokens.len() == 1 {
        if let Some(word) = tokens[0].as_word() {
            if let Some(dir) = parse_direction(word) {
                return Ok(ParsedCommand::Directional { direction: dir });
            }
        }
    }

    // Check for session / meta commands
    if let Some(word) = tokens[0].as_word() {
        match word {
            "save" if tokens.len() == 1 => {
                return Ok(ParsedCommand::Session(SessionCommand::Save));
            }
            "sync" if tokens.len() == 1 => {
                return Ok(ParsedCommand::Session(SessionCommand::Sync));
            }
            "control" if tokens.len() == 1 => {
                return Ok(ParsedCommand::Session(SessionCommand::Control));
            }
            "release" if tokens.len() == 1 => {
                return Ok(ParsedCommand::Session(SessionCommand::Release));
            }
            "places" if tokens.len() == 1 => {
                return Ok(ParsedCommand::Session(SessionCommand::Places));
            }
            "pace" if tokens.len() <= 2 => {
                let pace = match tokens.get(1) {
                    None => None,
                    Some(token) => Some(
                        token
                            .text()
                            .parse::<u64>()
                            .ok()
                            .filter(|ms| *ms <= 5000)
                            .ok_or("Use pace <milliseconds from 0 to 5000>.")?,
                    ),
                };
                return Ok(ParsedCommand::Session(SessionCommand::Pace(pace)));
            }
            "history" => {
                let rest: Vec<_> = tokens[1..].iter().map(|t| t.text()).collect();
                if rest.len() > 1 {
                    return Err("Use history [before-id]".into());
                }
                let before = rest.first().cloned();
                return Ok(ParsedCommand::Session(SessionCommand::History { before }));
            }
            "branch-history" | "branch_history" => {
                let rest: Vec<_> = tokens[1..].iter().map(|t| t.text()).collect();
                if rest.is_empty() || rest.len() > 2 {
                    return Err("Use branch-history <branch> [before-id]".into());
                }
                let branch = rest[0].clone();
                let before = rest.get(1).cloned();
                return Ok(ParsedCommand::Session(SessionCommand::BranchHistory {
                    branch,
                    before,
                }));
            }
            "note" | "bookmark" => {
                let text = if let Some(raw) = raw_line {
                    let (_, rest) = crate::word(raw.trim());
                    rest.to_string()
                } else {
                    tokens[1..]
                        .iter()
                        .map(|t| t.text())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                if text.trim().is_empty() {
                    return Err(format!("What {} would you like to make?", word));
                }
                return Ok(ParsedCommand::Session(SessionCommand::Note {
                    text,
                    is_bookmark: word == "bookmark",
                }));
            }
            "name" => {
                if tokens.len() < 3 {
                    return Err(
                        "Use name <place number> <new name> or name room <new name>.".into(),
                    );
                }
                let (target, name) = if let Some(raw) = raw_line {
                    let (_, rest) = crate::word(raw.trim());
                    let (t, n) = crate::word(rest);
                    (t.to_string(), n.to_string())
                } else {
                    let t = tokens[1].text();
                    let n = tokens[2..]
                        .iter()
                        .map(|t| t.text())
                        .collect::<Vec<_>>()
                        .join(" ");
                    (t, n)
                };
                return Ok(ParsedCommand::Session(SessionCommand::Name {
                    target,
                    name,
                }));
            }
            "annotate" => {
                if let Some(raw) = raw_line {
                    let (_, rest) = crate::word(raw.trim());
                    let (source, rest) = crate::word(rest);
                    let (audience, rest) = crate::word(rest);
                    let (category, rest) = crate::word(rest);
                    let (anchor, text) = crate::word(rest);
                    if source.is_empty()
                        || audience.is_empty()
                        || category.is_empty()
                        || anchor.is_empty()
                        || text.is_empty()
                    {
                        return Err(
                            "Use annotate <source> <audience> <category> <anchor> <text>".into(),
                        );
                    }
                    return Ok(ParsedCommand::Session(SessionCommand::Annotate {
                        source: source.into(),
                        audience: audience.into(),
                        category: category.into(),
                        anchor: anchor.into(),
                        text: text.into(),
                    }));
                } else {
                    let rest: Vec<_> = tokens[1..].iter().map(|t| t.text()).collect();
                    if rest.len() < 5 {
                        return Err(
                            "Use annotate <source> <audience> <category> <anchor> <text>".into(),
                        );
                    }
                    return Ok(ParsedCommand::Session(SessionCommand::Annotate {
                        source: rest[0].clone(),
                        audience: rest[1].clone(),
                        category: rest[2].clone(),
                        anchor: rest[3].clone(),
                        text: rest[4..].join(" "),
                    }));
                }
            }
            "wizard" => {
                let command = if let Some(raw) = raw_line {
                    let (_, rest) = crate::word(raw.trim());
                    rest.to_string()
                } else {
                    tokens[1..]
                        .iter()
                        .map(|t| t.text())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                if command.trim().is_empty() {
                    return Err("Use wizard <server developer command>.".into());
                }
                return Ok(ParsedCommand::Session(SessionCommand::Wizard { command }));
            }
            _ => {}
        }
    }

    // 2. Multi-word verb phrases at the beginning of the sentence
    let words: Vec<&str> = tokens.iter().filter_map(|t| t.as_word()).collect();
    if words.len() >= 2 {
        match (words[0], words[1]) {
            ("power", "strike") => {
                return parse_transitive_or_ditransitive(Verb::PowerStrike, &tokens[2..]);
            }
            ("magic", "bolt") => {
                return parse_transitive_or_ditransitive(Verb::MagicBolt, &tokens[2..]);
            }
            ("pick", "up") => {
                return parse_transitive_or_ditransitive(Verb::Take, &tokens[2..]);
            }
            ("put", "down") => {
                return parse_transitive_or_ditransitive(Verb::Drop, &tokens[2..]);
            }
            ("look", "at") => {
                return parse_transitive_or_ditransitive(Verb::Examine, &tokens[2..]);
            }
            ("look", "in" | "inside") => {
                let rest_np = parse_noun_phrase(&tokens[2..])?;
                return Ok(ParsedCommand::Ditransitive {
                    verb: Verb::Examine,
                    direct: rest_np,
                    preposition: Preposition::In,
                    indirect: NounPhrase::empty(),
                });
            }
            ("look", "under") => {
                let rest_np = parse_noun_phrase(&tokens[2..])?;
                return Ok(ParsedCommand::Ditransitive {
                    verb: Verb::Examine,
                    direct: rest_np,
                    preposition: Preposition::Under,
                    indirect: NounPhrase::empty(),
                });
            }
            ("look", "behind") => {
                let rest_np = parse_noun_phrase(&tokens[2..])?;
                return Ok(ParsedCommand::Ditransitive {
                    verb: Verb::Examine,
                    direct: rest_np,
                    preposition: Preposition::Behind,
                    indirect: NounPhrase::empty(),
                });
            }
            ("climb", "up") => {
                return Ok(ParsedCommand::Directional {
                    direction: Direction::Up,
                });
            }
            ("climb", "down") => {
                return Ok(ParsedCommand::Directional {
                    direction: Direction::Down,
                });
            }
            ("put", "on") => {
                return parse_transitive_or_ditransitive(Verb::Wear, &tokens[2..]);
            }
            ("take", "off") => {
                return parse_transitive_or_ditransitive(Verb::Remove, &tokens[2..]);
            }
            ("put" | "blow", "out") | ("turn", "off") => {
                return parse_transitive_or_ditransitive(Verb::Extinguish, &tokens[2..]);
            }
            ("turn", "on") => {
                return parse_transitive_or_ditransitive(Verb::Light, &tokens[2..]);
            }
            ("set", "down") => {
                return parse_transitive_or_ditransitive(Verb::Drop, &tokens[2..]);
            }
            ("get", "in" | "into") | ("go", "in" | "into" | "inside") => {
                return parse_transitive_or_ditransitive(Verb::Enter, &tokens[2..]);
            }
            ("look", "around") if words.len() == 2 => {
                return Ok(ParsedCommand::Intransitive { verb: Verb::Look });
            }
            ("sit", "down") | ("lie", "down") if words.len() == 2 => {
                return Ok(ParsedCommand::Intransitive {
                    verb: if words[0] == "sit" {
                        Verb::Sit
                    } else {
                        Verb::Sleep
                    },
                });
            }
            ("get", "out") | ("go", "out") if words.len() == 2 => {
                return Ok(ParsedCommand::Intransitive { verb: Verb::Exit });
            }
            ("talk", "to" | "with") | ("speak", "to" | "with") | ("chat", "with") => {
                return parse_transitive_or_ditransitive(Verb::Talk, &tokens[2..]);
            }
            ("go", "to" | "toward" | "towards") => {
                let np = parse_noun_phrase(&tokens[2..])?;
                return Ok(ParsedCommand::Transitive {
                    verb: Verb::Go,
                    direct: np,
                });
            }
            _ => {}
        }
    }

    // 3. Match leading verb
    let first_token = &tokens[0];
    let leading_verb = first_token.as_word().and_then(parse_verb);

    if let Some(verb) = leading_verb {
        let rest = &tokens[1..];

        // A bare verb. Whether it needs an object is the engine's business.
        if rest.is_empty() {
            return Ok(match verb {
                Verb::Stop => ParsedCommand::Stop,
                Verb::Again => ParsedCommand::Again,
                Verb::Help => ParsedCommand::Help { topic: None },
                verb => ParsedCommand::Intransitive { verb },
            });
        }

        // Special handling for Help with topic
        if verb == Verb::Help {
            let topic = rest.iter().map(|t| t.text()).collect::<Vec<_>>().join(" ");
            return Ok(ParsedCommand::Help { topic: Some(topic) });
        }

        // Special handling for Step <direction>
        if verb == Verb::Step {
            if rest.len() == 1 {
                if let Some(w) = rest[0].as_word() {
                    if let Some(dir) = parse_direction(w) {
                        return Ok(ParsedCommand::Step { direction: dir });
                    }
                }
            }
            return Err("Which direction would you like to step?".into());
        }

        // Special handling for Go <direction> or Go <place/noun>
        if verb == Verb::Go {
            if rest.len() == 1 {
                if let Some(w) = rest[0].as_word() {
                    if let Some(dir) = parse_direction(w) {
                        return Ok(ParsedCommand::Directional { direction: dir });
                    }
                }
            }
            let np = parse_noun_phrase(rest)?;
            return Ok(ParsedCommand::Transitive {
                verb: Verb::Go,
                direct: np,
            });
        }

        // Say <text>
        if verb == Verb::Say {
            let text = if let Some(raw) = raw_line {
                let (_, rest) = crate::word(raw.trim());
                rest.to_string()
            } else {
                rest.iter().map(|t| t.text()).collect::<Vec<_>>().join(" ")
            };
            return Ok(ParsedCommand::Say { text });
        }

        // General transitive / ditransitive / multi-transitive
        return parse_transitive_or_ditransitive(verb, rest);
    }

    // 4. If no leading verb, check if it's a noun phrase answering a clarification question
    if let Ok(np) = parse_noun_phrase(tokens) {
        return Ok(ParsedCommand::Clarification(np));
    }

    Err("I don't understand that sentence. Type help for things you can try.".into())
}

/// `<thing> to [the] <direction>` as `<direction> <thing>`.
fn where_it_lies(tokens: &[Token]) -> Option<Vec<Token>> {
    let to = tokens
        .iter()
        .position(|t| t.as_word() == Some("to"))
        .filter(|&i| i > 0)?;
    let rest: Vec<&Token> = tokens[to + 1..]
        .iter()
        .filter(|t| t.as_word() != Some("the"))
        .collect();
    let [direction] = rest.as_slice() else {
        return None;
    };
    direction.as_word().and_then(parse_direction)?;
    // After an article, if there is one: "the east door".
    let article = usize::from(matches!(tokens[0].as_word(), Some("the" | "a" | "an")));
    let mut reordered = tokens[..article].to_vec();
    reordered.push((*direction).clone());
    reordered.extend(tokens[article..to].iter().cloned());
    Some(reordered)
}

/// Helper to parse the remainder of a command as either:
/// 1. Ditransitive: `<direct> <prep> <indirect>` (e.g. "goblin with iron sword")
/// 2. MultiTransitive: `<noun1> and <noun2>` (e.g. "copper token and stone tablet")
/// 3. Transitive: `<noun>` (e.g. "brass lantern")
fn parse_transitive_or_ditransitive(verb: Verb, tokens: &[Token]) -> Result<ParsedCommand, String> {
    if tokens.is_empty() {
        return Err(format!("What do you want to {}?", verb.as_str()));
    }
    // A preposition right after the verb belongs to it: "knock on the door".
    let tokens = match tokens {
        [first, rest @ ..]
            if !rest.is_empty() && first.as_word().and_then(parse_preposition).is_some() =>
        {
            rest
        }
        _ => tokens,
    };
    // "the door to the east" names the door by where it lies, as "the east
    // door" does.
    let placed: Vec<Token>;
    let tokens = match where_it_lies(tokens) {
        Some(reordered) => {
            placed = reordered;
            &placed[..]
        }
        None => tokens,
    };

    // Search for a preposition that separates direct and indirect objects
    // We scan from index 1 to tokens.len() - 1 so both sides are non-empty
    let mut prep_split = None;
    for (i, token) in tokens.iter().enumerate().skip(1) {
        if let Some(word) = token.as_word() {
            if let Some(prep) = parse_preposition(word) {
                prep_split = Some((i, prep));
                break;
            }
        }
    }

    if let Some((idx, prep)) = prep_split {
        let direct_tokens = &tokens[..idx];
        let indirect_tokens = &tokens[idx + 1..];

        let direct = parse_noun_phrase(direct_tokens)?;
        let indirect = parse_noun_phrase(indirect_tokens)?;

        return Ok(ParsedCommand::Ditransitive {
            verb,
            direct,
            preposition: prep,
            indirect,
        });
    }

    // Check for conjunctions in the direct object (e.g. "sword and shield")
    let conjunction_groups = split_conjunction_phrases(tokens);
    if conjunction_groups.len() > 1 {
        let mut direct_list = Vec::new();
        for group in conjunction_groups {
            direct_list.push(parse_noun_phrase(&group)?);
        }
        return Ok(ParsedCommand::MultiTransitive { verb, direct_list });
    }

    // Ordinary transitive command
    let direct = parse_noun_phrase(tokens)?;
    Ok(ParsedCommand::Transitive { verb, direct })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::token::tokenize;

    #[test]
    fn directional() {
        let tokens = tokenize("north");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Directional {
                direction: Direction::North
            }
        );

        let tokens = tokenize("go east");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Directional {
                direction: Direction::East
            }
        );
    }

    #[test]
    fn transitive() {
        let tokens = tokenize("take the copper token");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(verb, Verb::Take);
                assert_eq!(direct.head.as_deref(), Some("token"));
                assert_eq!(direct.adjectives, vec!["copper"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn multiword_verbs() {
        let tokens = tokenize("pick up 3 arrows");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(verb, Verb::Take);
                assert_eq!(direct.quantity, Some(3));
                assert_eq!(direct.head.as_deref(), Some("arrows"));
            }
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("look at weathered stone tablet");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(verb, Verb::Examine);
                assert_eq!(direct.head.as_deref(), Some("tablet"));
                assert_eq!(direct.adjectives, vec!["weathered", "stone"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn ditransitive_combat() {
        let tokens = tokenize("attack the goblin with iron sword");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Ditransitive {
                verb,
                direct,
                preposition,
                indirect,
            } => {
                assert_eq!(verb, Verb::Attack);
                assert_eq!(direct.head.as_deref(), Some("goblin"));
                assert_eq!(preposition, Preposition::With);
                assert_eq!(indirect.head.as_deref(), Some("sword"));
                assert_eq!(indirect.adjectives, vec!["iron"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn ditransitive_manipulation() {
        let tokens = tokenize("take copper token from stone floor");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Ditransitive {
                verb,
                direct,
                preposition,
                indirect,
            } => {
                assert_eq!(verb, Verb::Take);
                assert_eq!(direct.head.as_deref(), Some("token"));
                assert_eq!(preposition, Preposition::From);
                assert_eq!(indirect.head.as_deref(), Some("floor"));
                assert_eq!(indirect.adjectives, vec!["stone"]);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn multi_transitive() {
        let tokens = tokenize("take sword and shield");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::MultiTransitive { verb, direct_list } => {
                assert_eq!(verb, Verb::Take);
                assert_eq!(direct_list.len(), 2);
                assert_eq!(direct_list[0].head.as_deref(), Some("sword"));
                assert_eq!(direct_list[1].head.as_deref(), Some("shield"));
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn clarification_noun_phrase() {
        let tokens = tokenize("the copper one");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Clarification(np) => {
                assert_eq!(np.adjectives, vec!["copper"]);
                assert!(np.is_one);
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn expanded_verbs() {
        let tokens = tokenize("put on the iron ring");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(verb, Verb::Wear);
                assert_eq!(direct.head.as_deref(), Some("ring"));
            }
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("take off cloak");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(verb, Verb::Remove);
                assert_eq!(direct.head.as_deref(), Some("cloak"));
            }
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("talk to goblin");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Transitive { verb, direct } => {
                assert_eq!(verb, Verb::Talk);
                assert_eq!(direct.head.as_deref(), Some("goblin"));
            }
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("ask goblin about key");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Ditransitive {
                verb,
                direct,
                preposition,
                indirect,
            } => {
                assert_eq!(verb, Verb::Ask);
                assert_eq!(direct.head.as_deref(), Some("goblin"));
                assert_eq!(preposition, Preposition::About);
                assert_eq!(indirect.head.as_deref(), Some("key"));
            }
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("diagnose");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Intransitive {
                verb: Verb::Diagnose
            }
        );

        let tokens = tokenize("give sword to goblin");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Ditransitive {
                verb,
                direct,
                preposition,
                indirect,
            } => {
                assert_eq!(verb, Verb::Give);
                assert_eq!(direct.head.as_deref(), Some("sword"));
                assert_eq!(preposition, Preposition::To);
                assert_eq!(indirect.head.as_deref(), Some("goblin"));
            }
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("put sword on floor");
        match match_sentence(&tokens).unwrap() {
            ParsedCommand::Ditransitive {
                verb,
                direct,
                preposition,
                indirect,
            } => {
                assert_eq!(verb, Verb::Put);
                assert_eq!(direct.head.as_deref(), Some("sword"));
                assert_eq!(preposition, Preposition::On);
                assert_eq!(indirect.head.as_deref(), Some("floor"));
            }
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn session_commands() {
        let tokens = tokenize("save");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::Save)
        );

        let tokens = tokenize("sync");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::Sync)
        );

        let tokens = tokenize("control");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::Control)
        );

        let tokens = tokenize("release");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::Release)
        );

        let tokens = tokenize("places");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::Places)
        );

        let tokens = tokenize("history");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::History { before: None })
        );

        let tokens = tokenize("history abc");
        assert_eq!(
            match_sentence(&tokens).unwrap(),
            ParsedCommand::Session(SessionCommand::History {
                before: Some("abc".into())
            })
        );

        let tokens = tokenize("note A mysterious inscription");
        assert_eq!(
            match_sentence_with_raw(&tokens, Some("note A mysterious inscription")).unwrap(),
            ParsedCommand::Session(SessionCommand::Note {
                text: "A mysterious inscription".into(),
                is_bookmark: false,
            })
        );

        let tokens = tokenize("name 1 Hearth of Echoes");
        assert_eq!(
            match_sentence_with_raw(&tokens, Some("name 1 Hearth of Echoes")).unwrap(),
            ParsedCommand::Session(SessionCommand::Name {
                target: "1".into(),
                name: "Hearth of Echoes".into(),
            })
        );

        let tokens = tokenize("wizard teleport 1 2 3");
        assert_eq!(
            match_sentence_with_raw(&tokens, Some("wizard teleport 1 2 3")).unwrap(),
            ParsedCommand::Session(SessionCommand::Wizard {
                command: "teleport 1 2 3".into(),
            })
        );
    }
}
