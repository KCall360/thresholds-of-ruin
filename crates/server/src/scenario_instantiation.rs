//! Install prepared creature definitions at the original construction boundary.
//! Authoring defaults and symbolic references have already been resolved.
use super::compiler::PreparedCreature;
use super::diagnostics::Origin;
use super::{fail, Failure};
use tor_simulation::{ActorId, Game};

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
