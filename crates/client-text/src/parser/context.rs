//! Conversational state, active pronoun referents, and disambiguation.
//!
//! Part of the resolver (`scope`, `matcher`, `context`), which is built and
//! tested but not yet used by the game: `adventure::Dialogue` still resolves
//! names, pronouns and clarification answers itself. See
//! section 3.3 of docs/if-parser-architecture.md.

use std::collections::VecDeque;

use super::{
    grammar::ParsedCommand,
    matcher::{match_noun_phrase, MatchResult, Referents},
    noun_phrase::NounPhrase,
    scope::{Entity, Scope},
};

#[derive(Clone, Debug)]
pub struct PendingDisambiguation {
    pub revision: u64,
    pub verb_phrase: String,
    pub candidates: Vec<Entity>,
}

#[derive(Clone, Debug, Default)]
pub struct ConversationContext {
    pub referents: Referents,
    pub pending: Option<PendingDisambiguation>,
    pub queue: VecDeque<ParsedCommand>,
    pub last_command: Option<ParsedCommand>,
}

impl ConversationContext {
    pub fn reset(&mut self) {
        self.referents = Referents::default();
        self.pending = None;
        self.queue.clear();
        self.last_command = None;
    }

    /// Updates the active pronoun referent when an entity is acted on or examined.
    pub fn mention(&mut self, entity: &Entity) {
        match entity {
            Entity::Actor { is_self, .. } => {
                if !*is_self {
                    self.referents.him = Some(entity.clone());
                }
            }
            Entity::Item { .. } | Entity::Door { .. } | Entity::Surface { .. } => {
                self.referents.it = Some(entity.clone());
            }
            Entity::Exit { .. } => {}
        }
    }

    /// Sets up a conversational disambiguation question.
    pub fn ask_disambiguation(
        &mut self,
        verb_phrase: &str,
        candidates: Vec<Entity>,
        revision: u64,
    ) -> String {
        let question = format_disambiguation_prompt(&candidates);
        self.pending = Some(PendingDisambiguation {
            revision,
            verb_phrase: verb_phrase.to_string(),
            candidates,
        });
        question
    }

    /// Attempts to resolve a pending disambiguation with a follow-up noun phrase.
    pub fn resolve_clarification(
        &mut self,
        np: &NounPhrase,
        revision: u64,
    ) -> Option<(String, Entity)> {
        let pending = self.pending.take()?;
        if pending.revision != revision {
            return None;
        }

        let temp_scope = Scope {
            entities: pending.candidates.clone(),
        };

        match match_noun_phrase(np, &temp_scope, &self.referents) {
            MatchResult::Single(chosen) => {
                self.mention(&chosen);
                Some((pending.verb_phrase, chosen))
            }
            _ => None,
        }
    }
}

/// Formats a list of candidate entities into a natural conversational question.
/// Example: "Which do you mean: the copper token or the silver token?"
pub fn format_disambiguation_prompt(candidates: &[Entity]) -> String {
    if candidates.is_empty() {
        return "You cannot see anything like that here.".into();
    }

    let names: Vec<String> = candidates
        .iter()
        .map(|c| format!("the {}", c.name()))
        .collect();

    match names.len() {
        1 => format!("Do you mean {}?", names[0]),
        2 => format!("Which do you mean: {} or {}?", names[0], names[1]),
        _ => {
            let all_but_last = names[..names.len() - 1].join(", ");
            let last = &names[names.len() - 1];
            format!("Which do you mean: {}, or {}?", all_but_last, last)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_disambiguation_prompt_two() {
        let candidates = vec![
            Entity::Item {
                id: 1,
                name: "copper token".into(),
                description: "".into(),
                quantity: 1,
                reachable: true,
                carried: false,
            },
            Entity::Item {
                id: 2,
                name: "silver token".into(),
                description: "".into(),
                quantity: 1,
                reachable: true,
                carried: false,
            },
        ];

        let question = format_disambiguation_prompt(&candidates);
        assert_eq!(
            question,
            "Which do you mean: the copper token or the silver token?"
        );
    }

    #[test]
    fn format_disambiguation_prompt_three() {
        let candidates = vec![
            Entity::Item {
                id: 1,
                name: "copper token".into(),
                description: "".into(),
                quantity: 1,
                reachable: true,
                carried: false,
            },
            Entity::Item {
                id: 2,
                name: "silver token".into(),
                description: "".into(),
                quantity: 1,
                reachable: true,
                carried: false,
            },
            Entity::Item {
                id: 3,
                name: "gold token".into(),
                description: "".into(),
                quantity: 1,
                reachable: true,
                carried: false,
            },
        ];

        let question = format_disambiguation_prompt(&candidates);
        assert_eq!(
            question,
            "Which do you mean: the copper token, the silver token, or the gold token?"
        );
    }
}
