//! Install prepared creature definitions at the original construction boundary.
//! Authoring defaults and symbolic references have already been resolved.
use super::compiler::PreparedCreature;
use super::{fail, in_declaration, Failure};
use tor_simulation::{ActorId, Game};

#[derive(Clone, Copy)]
pub(super) enum Origin<'a> {
    Character(u64),
    Actor { file: &'a str, region: u64, id: u64 },
    Item { file: &'a str, region: u64, id: u64 },
}

impl Origin<'_> {
    pub fn context(self, failure: Failure) -> Failure {
        self.context_at(failure, None)
    }

    fn context_at(self, failure: Failure, coordinates: Option<(usize, usize)>) -> Failure {
        let source = |file: &str| match coordinates {
            Some((line, column)) => format!("{file}:{line}:{column}"),
            None => file.into(),
        };
        let declaration = match self {
            Self::Character(id) => format!("{}: character {id}", source("scenario.toml")),
            Self::Actor { file, region, id } => {
                format!("{}: region {region}, actor {id}", source(file))
            }
            Self::Item { file, region, id } => {
                format!("{}: region {region}, item {id}", source(file))
            }
        };
        in_declaration(failure, declaration)
    }

    fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Character(_) => ("Character", "character"),
            Self::Actor { .. } => ("Actor", "actor"),
            Self::Item { .. } => ("Item", "item"),
        }
    }

    pub fn reference(
        self,
        failure: Failure,
        source: Option<&str>,
        field: &str,
        expected: &str,
    ) -> Failure {
        let (collection, id) = match self {
            Self::Character(id) => ("characters", id),
            Self::Actor { id, .. } => ("actors", id),
            Self::Item { id, .. } => ("items", id),
        };
        let location = source.and_then(|text| {
            super::diagnostics::reference_location(text, collection, id, &[field], expected)
        });
        self.context_at(failure, location.map(|location| location.coordinates()))
    }
}

enum ConfigurationError {
    Invalid(Failure),
    MissingAi(String),
}

impl From<Failure> for ConfigurationError {
    fn from(failure: Failure) -> Self {
        Self::Invalid(failure)
    }
}

pub(super) fn configure(
    game: &mut Game,
    id: u64,
    definition: PreparedCreature<'_>,
    origin: Origin<'_>,
    source: impl FnOnce() -> Option<std::sync::Arc<str>>,
) -> Result<(), Failure> {
    configure_inner(game, ActorId(id), definition, origin).map_err(|failure| match failure {
        ConfigurationError::Invalid(failure) => origin.context(failure),
        ConfigurationError::MissingAi(name) => {
            let (_, kind) = origin.names();
            origin.reference(
                fail(format!("Unknown {kind} AI profile {name:?}")),
                source().as_deref(),
                "ai",
                &name,
            )
        }
    })
}

fn configure_inner(
    game: &mut Game,
    id: ActorId,
    definition: PreparedCreature<'_>,
    origin: Origin<'_>,
) -> Result<(), ConfigurationError> {
    let (name, kind) = origin.names();
    if let Some(spec) = definition.combat {
        game.configure_combat(id, spec.into_owned())
            .map_err(|_| fail(format!("Invalid {kind} combat specification")))?;
    }
    if let Some(profile) = definition
        .control
        .profile()
        .map_err(|reference| ConfigurationError::MissingAi(reference.name.into()))?
    {
        game.configure_ai(id, profile.clone())
            .map_err(|_| fail("AI requires combat attributes"))?;
    }
    if let Some(body) = definition.body {
        if !body.cells.contains(&body.eye) {
            return Err(fail(format!("{name} body eye must be one of its cells")).into());
        }
        game.set_body(id, body.into_owned())
            .map_err(|_| fail(format!("{name} body does not fit")))?;
    }
    if let Some(velocity) = definition.velocity {
        game.set_actor_velocity(id, velocity)
            .map_err(|_| fail(format!("Invalid {kind} velocity")))?;
    }
    if let Some(asset) = definition.asset {
        game.set_actor_asset(id, Some(asset.into_owned()))
            .map_err(|_| fail(format!("Unknown {kind}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_context_locates_each_declaration_kind_and_preserves_the_failure() {
        for (origin, collection, context) in [
            (
                Origin::Character(3),
                "characters",
                "scenario.toml:3:11: character 3",
            ),
            (
                Origin::Actor {
                    file: "regions/2.toml",
                    region: 2,
                    id: 3,
                },
                "actors",
                "regions/2.toml:3:11: region 2, actor 3",
            ),
            (
                Origin::Item {
                    file: "regions/2.toml",
                    region: 2,
                    id: 3,
                },
                "items",
                "regions/2.toml:3:11: region 2, item 3",
            ),
        ] {
            let text = format!("[[{collection}]]\nid=3\narchetype=\"missing\"\n");
            let original = fail("Unknown archetype missing");
            let code = original.code;
            let failure = origin.reference(original, Some(&text), "archetype", "missing");
            assert_eq!(failure.code, code);
            assert_eq!(
                failure.message,
                format!("{context}: Unknown archetype missing")
            );
        }
    }

    #[test]
    fn unavailable_or_changed_source_keeps_declaration_context_without_coordinates() {
        let origin = Origin::Character(3);
        for source in [None, Some("[[characters]]\nid=3\nai=\"old\"\n")] {
            let failure = origin.reference(fail("Unknown AI profile"), source, "ai", "new");
            assert_eq!(
                failure.message,
                "scenario.toml: character 3: Unknown AI profile"
            );
        }
    }
}
