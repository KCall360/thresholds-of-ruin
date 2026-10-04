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
        let declaration = match self {
            Self::Character(id) => format!("scenario.toml: character {id}"),
            Self::Actor { file, region, id } => format!("{file}: region {region}, actor {id}"),
            Self::Item { file, region, id } => format!("{file}: region {region}, item {id}"),
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
}

pub(super) fn configure(
    game: &mut Game,
    id: u64,
    definition: PreparedCreature<'_>,
    origin: Origin<'_>,
) -> Result<(), Failure> {
    configure_inner(game, ActorId(id), definition, origin)
        .map_err(|failure| origin.context(failure))
}

fn configure_inner(
    game: &mut Game,
    id: ActorId,
    definition: PreparedCreature<'_>,
    origin: Origin<'_>,
) -> Result<(), Failure> {
    let (name, kind) = origin.names();
    if let Some(spec) = definition.combat {
        game.configure_combat(id, spec.into_owned())
            .map_err(|_| fail(format!("Invalid {kind} combat specification")))?;
    }
    if let Some(profile) = definition.control.profile(kind)? {
        game.configure_ai(id, profile.clone())
            .map_err(|_| fail("AI requires combat attributes"))?;
    }
    if let Some(body) = definition.body {
        if !body.cells.contains(&body.eye) {
            return Err(fail(format!("{name} body eye must be one of its cells")));
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
