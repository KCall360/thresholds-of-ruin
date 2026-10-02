//! Matching NounPhrases against entities in perceptual Scope.
//!
//! Part of the resolver (`scope`, `matcher`, `context`), which is built and
//! tested but not yet used by the game: `adventure::Dialogue` still resolves
//! names, pronouns and clarification answers itself. See
//! section 3.3 of docs/if-parser-architecture.md.

use super::{
    lexicon::Pronoun,
    noun_phrase::NounPhrase,
    scope::{Entity, Scope, SurfaceType},
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Referents {
    pub it: Option<Entity>,
    pub him: Option<Entity>,
    pub her: Option<Entity>,
    pub them: Vec<Entity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MatchResult {
    Single(Entity),
    Multiple(Vec<Entity>),
    All(Vec<Entity>),
    None,
}

pub fn match_noun_phrase(np: &NounPhrase, scope: &Scope, referents: &Referents) -> MatchResult {
    // 1. Handle Pronouns
    if let Some(pronoun) = np.pronoun {
        return match pronoun {
            Pronoun::It => referents
                .it
                .as_ref()
                .filter(|e| scope.entities.contains(e))
                .cloned()
                .map_or(MatchResult::None, MatchResult::Single),
            Pronoun::Him => referents
                .him
                .as_ref()
                .filter(|e| scope.entities.contains(e))
                .cloned()
                .map_or(MatchResult::None, MatchResult::Single),
            Pronoun::Her => referents
                .her
                .as_ref()
                .filter(|e| scope.entities.contains(e))
                .cloned()
                .map_or(MatchResult::None, MatchResult::Single),
            Pronoun::Them => {
                let valid: Vec<_> = referents
                    .them
                    .iter()
                    .filter(|e| scope.entities.contains(e))
                    .cloned()
                    .collect();
                if valid.is_empty() {
                    MatchResult::None
                } else if valid.len() == 1 {
                    MatchResult::Single(valid.into_iter().next().unwrap())
                } else {
                    MatchResult::All(valid)
                }
            }
        };
    }

    // 2. Handle "all" or "everything"
    if np.all {
        let mut candidates = scope.entities.clone();
        if let Some(except) = &np.except {
            let excluded = match_noun_phrase(except, scope, referents);
            let exclude_set: Vec<Entity> = match excluded {
                MatchResult::Single(e) => vec![e],
                MatchResult::Multiple(list) | MatchResult::All(list) => list,
                MatchResult::None => Vec::new(),
            };
            candidates.retain(|c| !exclude_set.contains(c));
        }
        return if candidates.is_empty() {
            MatchResult::None
        } else {
            MatchResult::All(candidates)
        };
    }

    // 3. Match against Scope Entities
    let mut matches = Vec::new();

    for entity in &scope.entities {
        if entity_matches_noun_phrase(entity, np) {
            matches.push(entity.clone());
        }
    }

    // 4. Handle Ordinals (e.g. "the second door")
    if let Some(ordinal) = np.ordinal {
        if ordinal > 0 && ordinal <= matches.len() {
            return MatchResult::Single(matches.remove(ordinal - 1));
        }
        return MatchResult::None;
    }

    match matches.len() {
        0 => MatchResult::None,
        1 => MatchResult::Single(matches.into_iter().next().unwrap()),
        _ => MatchResult::Multiple(matches),
    }
}

fn entity_matches_noun_phrase(entity: &Entity, np: &NounPhrase) -> bool {
    let name_lower = entity.name().to_lowercase();
    let name_words: Vec<&str> = name_lower.split_whitespace().collect();

    // Surface special matching: floor, wall, walls, ceiling
    if let Entity::Surface {
        surface_type,
        materials,
    } = entity
    {
        let surface_word = match surface_type {
            SurfaceType::Floor => "floor",
            SurfaceType::Wall => "wall",
            SurfaceType::Ceiling => "ceiling",
        };

        let head_match = np.head.as_deref().map_or(np.is_one, |h| {
            h == surface_word
                || (surface_word == "wall" && h == "walls")
                || (surface_word == "floor" && h == "ground")
        });

        if !head_match {
            return false;
        }

        // Check if all adjectives match materials
        for adj in &np.adjectives {
            let adj_lower = adj.to_lowercase();
            if !materials
                .iter()
                .any(|m| m.to_lowercase().contains(&adj_lower))
            {
                return false;
            }
        }

        return true;
    }

    // Check head noun
    if let Some(head) = &np.head {
        let head_lower = head.to_lowercase();
        let head_matched = name_words
            .iter()
            .any(|w| *w == head_lower || (*w).strip_suffix('s') == Some(&head_lower));
        if !head_matched {
            return false;
        }
    }

    // Check adjectives
    for adj in &np.adjectives {
        let adj_lower = adj.to_lowercase();
        let adj_matched = name_words.iter().any(|w| *w == adj_lower);
        if !adj_matched {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::noun_phrase::parse_noun_phrase;
    use crate::parser::token::tokenize;

    #[test]
    fn match_single_entity() {
        let scope = Scope {
            entities: vec![
                Entity::Item {
                    id: 1,
                    name: "copper token".into(),
                    description: "A small copper disc.".into(),
                    quantity: 1,
                    reachable: true,
                    carried: false,
                },
                Entity::Item {
                    id: 2,
                    name: "stone tablet".into(),
                    description: "A weathered tablet.".into(),
                    quantity: 1,
                    reachable: false,
                    carried: false,
                },
            ],
        };
        let referents = Referents::default();

        let tokens = tokenize("copper token");
        let np = parse_noun_phrase(&tokens).unwrap();
        match match_noun_phrase(&np, &scope, &referents) {
            MatchResult::Single(Entity::Item { id, .. }) => assert_eq!(id, 1),
            other => panic!("Unexpected: {other:?}"),
        }

        let tokens = tokenize("tablet");
        let np = parse_noun_phrase(&tokens).unwrap();
        match match_noun_phrase(&np, &scope, &referents) {
            MatchResult::Single(Entity::Item { id, .. }) => assert_eq!(id, 2),
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn match_multiple_ambiguous() {
        let scope = Scope {
            entities: vec![
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
            ],
        };
        let referents = Referents::default();

        let tokens = tokenize("token");
        let np = parse_noun_phrase(&tokens).unwrap();
        match match_noun_phrase(&np, &scope, &referents) {
            MatchResult::Multiple(list) => assert_eq!(list.len(), 2),
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn match_ordinal() {
        let scope = Scope {
            entities: vec![
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
            ],
        };
        let referents = Referents::default();

        let tokens = tokenize("the second token");
        let np = parse_noun_phrase(&tokens).unwrap();
        match match_noun_phrase(&np, &scope, &referents) {
            MatchResult::Single(Entity::Item { id, .. }) => assert_eq!(id, 2),
            other => panic!("Unexpected: {other:?}"),
        }
    }

    #[test]
    fn match_pronoun_it() {
        let item = Entity::Item {
            id: 1,
            name: "copper token".into(),
            description: "".into(),
            quantity: 1,
            reachable: true,
            carried: false,
        };
        let scope = Scope {
            entities: vec![item.clone()],
        };
        let referents = Referents {
            it: Some(item.clone()),
            ..Default::default()
        };

        let tokens = tokenize("it");
        let np = parse_noun_phrase(&tokens).unwrap();
        match match_noun_phrase(&np, &scope, &referents) {
            MatchResult::Single(Entity::Item { id, .. }) => assert_eq!(id, 1),
            other => panic!("Unexpected: {other:?}"),
        }
    }
}
