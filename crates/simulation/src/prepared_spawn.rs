//! Immutable, catalog-normalized construction inputs shared by ordinary setup
//! and authoritative population. These are recipes, never live entity state.
use crate::creatures::{record::BuildRecord, BuildError, CreatureBuild};
use crate::{ActorId, AnatomySpec, BodySpec, CreatureIdentity, Game, GameError, ItemId, ItemSpec};
use serde::{Deserialize, Serialize};
use tor_world::Location;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedActor {
    pub build: Option<BuildRecord>,
    pub nutrition: crate::nutrition::NutritionRequirement,
    pub identity: Option<CreatureIdentity>,
    pub anatomy: Option<AnatomySpec>,
    pub body: Option<BodySpec>,
    pub ai: Option<crate::ai::AiProfile>,
    pub known_identities: Vec<String>,
    pub velocity: Option<[i64; 3]>,
    pub asset: Option<String>,
}

impl<'de> Deserialize<'de> for PreparedActor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            build: Option<BuildRecord>,
            nutrition: crate::nutrition::NutritionRequirement,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            identity: Option<CreatureIdentity>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            anatomy: Option<AnatomySpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            body: Option<BodySpec>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            ai: Option<crate::ai::AiProfile>,
            known_identities: crate::bounded::Bounded<String, 4096>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            velocity: Option<[i64; 3]>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            build: fields.build,
            nutrition: fields.nutrition,
            identity: fields.identity,
            anatomy: fields.anatomy,
            body: fields.body,
            ai: fields.ai,
            known_identities: fields.known_identities.0,
            velocity: fields.velocity,
            asset: fields.asset,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedSpawnError {
    Anatomy(GameError),
    Knowledge(GameError),
    Build(BuildError),
    Creature(GameError),
    Nutrition(GameError),
    Ai(GameError),
    Body(GameError),
    Velocity(GameError),
    Asset(GameError),
}

impl CreatureBuild {
    /// Reseed an immutable prototype using the existing HD reconstruction path.
    /// Recorded prototype seeds are not spawn decisions. All authored ownership,
    /// selected forms, binding, templates and future growth remain unchanged.
    pub fn instantiate_seeded(&self, seed: u64) -> Result<Self, BuildError> {
        let ledger = crate::progression::HdLedger::seeded(
            self.ledger()
                .entries()
                .iter()
                .map(|die| die.source())
                .collect(),
            seed,
        )
        .map_err(|_| BuildError::InvalidDefinition)?;
        let mut result = Self::from_recorded(
            self.content().clone(),
            self.species_id().clone(),
            self.initial_attributes(),
            ledger,
            self.binding(),
            self.advancement().clone(),
            self.template_ids().to_vec(),
        )?;
        result.set_future_growth(self.future_growth().clone())?;
        Ok(result)
    }
}

impl PreparedActor {
    pub(crate) fn valid(&self, game: &Game) -> bool {
        self.build.is_some() == self.identity.is_some()
            && (self.nutrition == crate::nutrition::NutritionRequirement::Exempt
                || (self.build.is_some()
                    && crate::nutrition::Nutrition::initial(
                        self.nutrition,
                        game.content.rules().document().nutrition,
                    )
                    .is_some()))
            && !(self.build.is_some() && self.anatomy.is_some())
            && self.identity.as_ref().is_none_or(CreatureIdentity::valid)
            && self
                .build
                .as_ref()
                .is_none_or(|build| build.restore(game.content.clone()).is_ok())
            && self
                .anatomy
                .as_ref()
                .is_none_or(|a| a.valid(game.content.rules()))
            && self.body.as_ref().is_none_or(BodySpec::valid)
            && self
                .ai
                .as_ref()
                .is_none_or(|ai| self.build.is_some() && ai.valid())
            && self.known_identities.len() <= 4096
            && self
                .known_identities
                .iter()
                .all(|s| !s.is_empty() && s.len() <= 80 && !s.chars().any(char::is_control))
            && self.velocity.is_none_or(|v| {
                v.iter()
                    .all(|n| n.unsigned_abs() <= crate::physics::TERMINAL as u64)
            })
            && self
                .asset
                .as_ref()
                .is_none_or(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
    }
}

impl Game {
    /// Install into a transaction candidate through the same ordinary constructors
    /// as authored actors. The caller owns publication/rollback of the candidate.
    pub fn configure_prepared_actor(
        &mut self,
        id: ActorId,
        definition: &PreparedActor,
        seed: u64,
    ) -> Result<(), PreparedSpawnError> {
        if let Some(anatomy) = &definition.anatomy {
            self.configure_anatomy(id, anatomy.clone())
                .map_err(PreparedSpawnError::Anatomy)?;
        }
        for identity in &definition.known_identities {
            self.learn_identity(id, identity)
                .map_err(PreparedSpawnError::Knowledge)?;
        }
        if let Some(record) = &definition.build {
            let prototype = record
                .restore(self.content.clone())
                .map_err(PreparedSpawnError::Build)?;
            let build = prototype
                .instantiate_seeded(seed)
                .map_err(PreparedSpawnError::Build)?;
            let identity = definition
                .identity
                .clone()
                .ok_or(PreparedSpawnError::Build(BuildError::InvalidDefinition))?;
            self.configure_creature(id, identity, build)
                .map_err(PreparedSpawnError::Creature)?;
            self.configure_nutrition(id, definition.nutrition)
                .map_err(PreparedSpawnError::Nutrition)?;
        }
        if let Some(ai) = &definition.ai {
            self.configure_ai(id, ai.clone())
                .map_err(PreparedSpawnError::Ai)?;
        }
        if let Some(body) = &definition.body {
            self.set_body(id, body.clone())
                .map_err(PreparedSpawnError::Body)?;
        }
        if let Some(velocity) = definition.velocity {
            self.set_actor_velocity(id, velocity)
                .map_err(PreparedSpawnError::Velocity)?;
        }
        if let Some(asset) = &definition.asset {
            self.set_actor_asset(id, Some(asset.clone()))
                .map_err(PreparedSpawnError::Asset)?;
        }
        Ok(())
    }

    /// Allocate from the ordinary item namespace, retaining its checked ownership,
    /// quantity, definition and lifecycle validation. No secondary allocator.
    pub fn spawn_item_stack(
        &mut self,
        at: Location,
        owner: Option<ActorId>,
        quantity: u64,
        spec: ItemSpec,
    ) -> Result<ItemId, GameError> {
        let id = self.next_item_id;
        self.place_item_stack(id, at, owner, quantity, spec)
    }
}
