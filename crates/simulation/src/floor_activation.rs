//! Once-only authoritative population at an accepted character entry. Geometry
//! preparation registers immutable inputs only; it never rolls or spawns.
use crate::prepared_spawn::PreparedActor;
use crate::{ActorId, Game, GameError, ItemId, ItemLocation, ItemSpec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use tor_world::{Location, RegionId, Shared};

pub const MAX_PROFILES: usize = 32;
pub const MAX_STAGES: usize = 32;
pub const MAX_CHOICES: usize = 64;
pub const MAX_ATTEMPTS: u16 = 256;
pub const MAX_SCOPE_MEMBERS: usize = 64;
pub const MAX_SCOPE_CELLS: usize = 65_536;
pub const MAX_SCOPES: usize = 1_000_000;
pub const MAX_RETAINED_DECISIONS: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCapabilities {
    pub npc_stair_traversal: bool,
}
impl<'de> Deserialize<'de> for ScenarioCapabilities {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            npc_stair_traversal: bool,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            npc_stair_traversal: fields.npc_stair_traversal,
        })
    }
}

impl Default for ScenarioCapabilities {
    fn default() -> Self {
        Self {
            npc_stair_traversal: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioPolicy {
    capabilities: ScenarioCapabilities,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    selected_character: Option<ActorId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationPlan {
    pub protagonist: ActorId,
    #[serde(deserialize_with = "read_profiles")]
    pub profiles: BTreeMap<String, PopulationProfile>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationProfile {
    pub version: u32,
    #[serde(deserialize_with = "read_stages")]
    pub stages: Vec<PopulationStage>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationStage {
    pub id: String,
    pub version: u32,
    pub minimum_depth: u32,
    pub count: [u16; 2],
    pub population: PopulationContent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationContent {
    Items {
        #[serde(deserialize_with = "read_item_choices")]
        choices: Vec<PreparedItemChoice>,
    },
    Actors {
        #[serde(deserialize_with = "read_actor_choices")]
        choices: Vec<PreparedActorChoice>,
        avoid_protagonist_sight: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedItemChoice {
    pub content: String,
    pub quantity: u64,
    pub spec: ItemSpec,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedActorChoice {
    pub content: String,
    pub turn_ticks: u64,
    pub actor: PreparedActor,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedScope {
    pub scope: String,
    pub depth: u32,
    pub profile: String,
    #[serde(deserialize_with = "read_members")]
    pub members: BTreeSet<RegionId>,
    #[serde(deserialize_with = "read_cells")]
    pub cells: Vec<Location>,
    #[serde(deserialize_with = "read_protected")]
    pub protected: BTreeSet<Location>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryCause {
    Initial,
    Movement,
    Physics,
    Teleport,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FloorActivationRecord {
    pub scope: String,
    pub entrant: ActorId,
    pub tick: u64,
    pub ordinal: u64,
    pub cause: EntryCause,
    pub profile: String,
    pub profile_version: u32,
    #[serde(deserialize_with = "read_decisions")]
    pub decisions: Vec<StageDecision>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageDecision {
    pub id: String,
    pub version: u32,
    pub result: StageResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageResult {
    Skipped {
        reason: SkipReason,
    },
    Evaluated {
        requested: u16,
        #[serde(deserialize_with = "read_attempts")]
        attempts: Vec<PlacementDecision>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    BelowMinimumDepth,
    NoEligibleCell,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlacementDecision {
    Placed {
        content: String,
        #[serde(deserialize_with = "crate::bounded::location")]
        location: Location,
        entity: PopulationEntity,
    },
    Skipped {
        content: String,
        reason: SkipReason,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationEntity {
    Actor { id: ActorId },
    Item { id: ItemId },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivationState {
    plan: Shared<PopulationPlan>,
    #[serde(deserialize_with = "read_scopes")]
    scopes: Shared<BTreeMap<String, Shared<PreparedScope>>>,
    #[serde(deserialize_with = "read_member_index")]
    members: Shared<BTreeMap<RegionId, String>>,
    #[serde(deserialize_with = "read_floors")]
    pub(crate) floors: Shared<BTreeMap<String, Shared<FloorActivationRecord>>>,
    #[serde(deserialize_with = "read_order")]
    order: Shared<Vec<String>>,
    decision_count: usize,
}

impl<'de> Deserialize<'de> for ScenarioPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            capabilities: ScenarioCapabilities,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            selected_character: Option<ActorId>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            capabilities: fields.capabilities,
            selected_character: fields.selected_character,
        })
    }
}
impl<'de> Deserialize<'de> for PopulationPlan {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            protagonist: ActorId,
            #[serde(deserialize_with = "read_profiles")]
            profiles: BTreeMap<String, PopulationProfile>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            protagonist: fields.protagonist,
            profiles: fields.profiles,
        })
    }
}
impl<'de> Deserialize<'de> for PopulationProfile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            version: u32,
            #[serde(deserialize_with = "read_stages")]
            stages: Vec<PopulationStage>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            version: fields.version,
            stages: fields.stages,
        })
    }
}
impl<'de> Deserialize<'de> for PopulationStage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: String,
            version: u32,
            minimum_depth: u32,
            count: [u16; 2],
            population: PopulationContent,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            version: fields.version,
            minimum_depth: fields.minimum_depth,
            count: fields.count,
            population: fields.population,
        })
    }
}
impl<'de> Deserialize<'de> for PreparedItemChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            content: String,
            quantity: u64,
            spec: ItemSpec,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            content: fields.content,
            quantity: fields.quantity,
            spec: fields.spec,
        })
    }
}
impl<'de> Deserialize<'de> for PreparedActorChoice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            content: String,
            turn_ticks: u64,
            actor: PreparedActor,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            content: fields.content,
            turn_ticks: fields.turn_ticks,
            actor: fields.actor,
        })
    }
}
impl<'de> Deserialize<'de> for PreparedScope {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            scope: String,
            depth: u32,
            profile: String,
            #[serde(deserialize_with = "read_members")]
            members: BTreeSet<RegionId>,
            #[serde(deserialize_with = "read_cells")]
            cells: Vec<Location>,
            #[serde(deserialize_with = "read_protected")]
            protected: BTreeSet<Location>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            scope: fields.scope,
            depth: fields.depth,
            profile: fields.profile,
            members: fields.members,
            cells: fields.cells,
            protected: fields.protected,
        })
    }
}
impl<'de> Deserialize<'de> for FloorActivationRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            scope: String,
            entrant: ActorId,
            tick: u64,
            ordinal: u64,
            cause: EntryCause,
            profile: String,
            profile_version: u32,
            #[serde(deserialize_with = "read_decisions")]
            decisions: Vec<StageDecision>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            scope: fields.scope,
            entrant: fields.entrant,
            tick: fields.tick,
            ordinal: fields.ordinal,
            cause: fields.cause,
            profile: fields.profile,
            profile_version: fields.profile_version,
            decisions: fields.decisions,
        })
    }
}
impl<'de> Deserialize<'de> for StageDecision {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: String,
            version: u32,
            result: StageResult,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            version: fields.version,
            result: fields.result,
        })
    }
}
impl<'de> Deserialize<'de> for ActivationState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            plan: Shared<PopulationPlan>,
            #[serde(deserialize_with = "read_scopes")]
            scopes: Shared<BTreeMap<String, Shared<PreparedScope>>>,
            #[serde(deserialize_with = "read_member_index")]
            members: Shared<BTreeMap<RegionId, String>>,
            #[serde(deserialize_with = "read_floors")]
            floors: Shared<BTreeMap<String, Shared<FloorActivationRecord>>>,
            #[serde(deserialize_with = "read_order")]
            order: Shared<Vec<String>>,
            decision_count: usize,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            plan: fields.plan,
            scopes: fields.scopes,
            members: fields.members,
            floors: fields.floors,
            order: fields.order,
            decision_count: fields.decision_count,
        })
    }
}

fn bounded_map<'de, K, V, D, const N: usize>(deserializer: D) -> Result<BTreeMap<K, V>, D::Error>
where
    K: Deserialize<'de> + Ord,
    V: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    struct Map<K, V, const N: usize>(std::marker::PhantomData<(K, V)>);
    impl<'de, K: Deserialize<'de> + Ord, V: Deserialize<'de>, const N: usize>
        serde::de::Visitor<'de> for Map<K, V, N>
    {
        type Value = BTreeMap<K, V>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} unique map entries")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if map.size_hint().is_some_and(|size| size > N) {
                return Err(A::Error::custom("collection exceeds limit"));
            }
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                if result.len() == N || result.insert(key, value).is_some() {
                    return Err(A::Error::custom("excessive or duplicate map entry"));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Map::<K, V, N>(std::marker::PhantomData))
}
fn read_stages<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<PopulationStage>, D::Error> {
    Ok(crate::bounded::Bounded::<PopulationStage, MAX_STAGES>::deserialize(d)?.0)
}
fn read_item_choices<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PreparedItemChoice>, D::Error> {
    Ok(crate::bounded::Bounded::<PreparedItemChoice, MAX_CHOICES>::deserialize(d)?.0)
}
fn read_actor_choices<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PreparedActorChoice>, D::Error> {
    Ok(crate::bounded::Bounded::<PreparedActorChoice, MAX_CHOICES>::deserialize(d)?.0)
}
fn read_decisions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<StageDecision>, D::Error> {
    Ok(crate::bounded::Bounded::<StageDecision, MAX_STAGES>::deserialize(d)?.0)
}
fn read_attempts<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<PlacementDecision>, D::Error> {
    Ok(crate::bounded::Bounded::<PlacementDecision, { MAX_ATTEMPTS as usize }>::deserialize(d)?.0)
}

fn read_profiles<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, PopulationProfile>, D::Error> {
    bounded_map::<_, _, _, MAX_PROFILES>(d)
}
fn read_scopes<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<String, Shared<PreparedScope>>>, D::Error> {
    bounded_map::<_, _, _, MAX_SCOPES>(d).map(Shared::new)
}
fn read_member_index<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<RegionId, String>>, D::Error> {
    bounded_map::<_, _, _, { MAX_SCOPES * MAX_SCOPE_MEMBERS }>(d).map(Shared::new)
}
fn read_floors<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<BTreeMap<String, Shared<FloorActivationRecord>>>, D::Error> {
    bounded_map::<_, _, _, MAX_SCOPES>(d).map(Shared::new)
}
fn read_order<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Shared<Vec<String>>, D::Error> {
    Ok(Shared::new(
        crate::bounded::Bounded::<String, MAX_SCOPES>::deserialize(d)?.0,
    ))
}
fn read_members<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeSet<RegionId>, D::Error> {
    let values = crate::bounded::Bounded::<RegionId, MAX_SCOPE_MEMBERS>::deserialize(d)?.0;
    let count = values.len();
    let set: BTreeSet<_> = values.into_iter().collect();
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate scope member"));
    }
    Ok(set)
}
fn read_cells<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Location>, D::Error> {
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Cell(#[serde(deserialize_with = "crate::bounded::location")] Location);
    Ok(
        crate::bounded::Bounded::<Cell, MAX_SCOPE_CELLS>::deserialize(d)?
            .0
            .into_iter()
            .map(|c| c.0)
            .collect(),
    )
}
fn read_protected<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeSet<Location>, D::Error> {
    let values = read_cells(d)?;
    let count = values.len();
    let set: BTreeSet<_> = values.into_iter().collect();
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate protected cell"));
    }
    Ok(set)
}

fn invalid() -> GameError {
    GameError::Rule(crate::abilities::RuleError::InvalidDefinition)
}
fn label(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
}
impl PopulationPlan {
    fn valid(&self, game: &Game) -> bool {
        self.protagonist.0 != 0
            && !self.profiles.is_empty()
            && self.profiles.len() <= MAX_PROFILES
            && self.profiles.iter().all(|(id, profile)| {
                let mut names = BTreeSet::new();
                label(id)
                    && profile.version == 1
                    && profile.stages.len() <= MAX_STAGES
                    && profile
                        .stages
                        .iter()
                        .map(|stage| usize::from(stage.count[1]))
                        .sum::<usize>()
                        <= usize::from(MAX_ATTEMPTS)
                    && profile.stages.iter().all(|stage| {
                        label(&stage.id)
                            && names.insert(&stage.id)
                            && stage.version == 1
                            && stage.minimum_depth > 0
                            && stage.count[0] <= stage.count[1]
                            && stage.count[1] <= MAX_ATTEMPTS
                            && match &stage.population {
                                PopulationContent::Items { choices } => {
                                    !choices.is_empty()
                                        && choices.len() <= MAX_CHOICES
                                        && choices.iter().all(|c| {
                                            label(&c.content)
                                                && c.quantity > 0
                                                && c.spec.valid()
                                                && (c.spec.stackable || c.quantity == 1)
                                                && c.spec
                                                    .validate_rules(game.content.rules())
                                                    .is_ok()
                                        })
                                }
                                PopulationContent::Actors { choices, .. } => {
                                    !choices.is_empty()
                                        && choices.len() <= MAX_CHOICES
                                        && choices.iter().all(|c| {
                                            label(&c.content)
                                                && c.turn_ticks > 0
                                                && c.actor.valid(game)
                                        })
                                }
                            }
                    })
            })
    }
}
impl PreparedScope {
    fn valid(&self) -> bool {
        label(&self.scope)
            && label(&self.profile)
            && self.depth > 0
            && self
                .scope
                .rsplit_once('/')
                .and_then(|(_, depth)| depth.parse::<u32>().ok())
                == Some(self.depth)
            && !self.members.is_empty()
            && self.members.len() <= MAX_SCOPE_MEMBERS
            && self.members.iter().all(|id| id.0 != 0)
            && !self.cells.is_empty()
            && self.cells.len() <= MAX_SCOPE_CELLS
            && self.cells.windows(2).all(|w| w[0] < w[1])
            && self
                .cells
                .iter()
                .all(|at| self.members.contains(&at.region))
            && self.protected.len() <= MAX_SCOPE_CELLS
            && self
                .protected
                .iter()
                .all(|at| self.members.contains(&at.region))
    }
}

// Versioned, length-delimited domain separation from immutable Game.seed. This
// FNV-1a key mixer feeds the existing splitmix word stream, never rules_rng. No
// package hash, entity ID, speculative load tick or worker order is an input.
fn stream(
    seed: u64,
    scope_id: &str,
    profile_id: &str,
    profile: &PopulationProfile,
    stage: &PopulationStage,
    ordinal: u64,
    purpose: &str,
) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    add(b"tor-population-v1");
    add(&seed.to_le_bytes());
    add(scope_id.as_bytes());
    add(profile_id.as_bytes());
    add(&profile.version.to_le_bytes());
    add(stage.id.as_bytes());
    add(&stage.version.to_le_bytes());
    add(&ordinal.to_le_bytes());
    add(purpose.as_bytes());
    hash
}
fn index(state: &mut u64, n: usize) -> usize {
    let n = n as u64;
    let threshold = n.wrapping_neg() % n;
    loop {
        let word = crate::dice::random_word(state);
        if word >= threshold {
            return (word % n) as usize;
        }
    }
}

/// A private host execution may require no-time-advance residency settlement.
/// The value is not a completed external action until the host drains the
/// boundary and recomputes the final decision. Standalone Game APIs never yield.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateStep<T> {
    Settled(T),
    AwaitingRegions(T),
}

impl Game {
    fn candidate_step<T>(&self, value: T) -> CandidateStep<T> {
        if self.population_entry_pending() {
            CandidateStep::AwaitingRegions(value)
        } else {
            CandidateStep::Settled(value)
        }
    }
    pub fn act_for_host(
        &mut self,
        id: ActorId,
        action: crate::Action,
    ) -> Result<CandidateStep<crate::ActionOutcome>, GameError> {
        if self.population_entry_pending() || self.host_region_continuation {
            return Err(invalid());
        }
        self.host_region_continuation = true;
        let result = self.act(id, action);
        self.host_region_continuation = false;
        result.map(|value| self.candidate_step(value))
    }
    pub fn execute_next_intention_for_host(
        &mut self,
    ) -> Option<CandidateStep<crate::IntentionExecution>> {
        if self.population_entry_pending() || self.host_region_continuation {
            return None;
        }
        self.host_region_continuation = true;
        let result = self.execute_next_intention();
        self.host_region_continuation = false;
        result.map(|value| self.candidate_step(value))
    }
    pub fn cancel_intention_with_step_for_host(
        &mut self,
        actor: ActorId,
        id: crate::IntentionId,
        step: Option<tor_world::Direction>,
    ) -> Result<CandidateStep<()>, GameError> {
        if self.population_entry_pending() || self.host_region_continuation {
            return Err(invalid());
        }
        self.host_region_continuation = true;
        let result = self.cancel_intention_with_step(actor, id, step);
        self.host_region_continuation = false;
        result.map(|value| self.candidate_step(value))
    }
    pub fn teleport_for_host(
        &mut self,
        actor: ActorId,
        at: Location,
    ) -> Result<CandidateStep<()>, GameError> {
        if self.population_entry_pending() || self.host_region_continuation {
            return Err(invalid());
        }
        self.host_region_continuation = true;
        let result = self.teleport(actor, at);
        self.host_region_continuation = false;
        result.map(|value| self.candidate_step(value))
    }
    pub(crate) fn activation_item_births_at(&self, tick: u64) -> BTreeSet<ItemId> {
        let mut items = BTreeSet::new();
        if let Some(state) = &self.floor_activation {
            for key in state.order.iter().rev() {
                let record = &state.floors[key];
                if record.tick != tick {
                    break;
                }
                for decision in &record.decisions {
                    if let StageResult::Evaluated { attempts, .. } = &decision.result {
                        for attempt in attempts {
                            if let PlacementDecision::Placed {
                                entity: PopulationEntity::Item { id },
                                ..
                            } = attempt
                            {
                                items.insert(*id);
                            }
                        }
                    }
                }
            }
        }
        items
    }
}

impl Game {
    pub fn configure_scenario_policy(
        &mut self,
        capabilities: ScenarioCapabilities,
        selected_character: Option<ActorId>,
    ) -> Result<(), GameError> {
        if self.tick != 0
            || !self.navigation.is_empty()
            || selected_character.is_some_and(|id| id.0 == 0)
            || self.scenario_policy != ScenarioPolicy::default()
        {
            return Err(invalid());
        }
        self.scenario_policy = ScenarioPolicy {
            capabilities,
            selected_character,
        };
        Ok(())
    }
    pub(crate) fn can_use_abstract_stairs(&self, id: ActorId) -> bool {
        self.scenario_policy.capabilities.npc_stair_traversal
            || self.scenario_policy.selected_character == Some(id)
    }
    pub fn configure_population(&mut self, plan: PopulationPlan) -> Result<(), GameError> {
        if self.tick != 0
            || self.floor_activation.is_some()
            || !plan.valid(self)
            || self.scenario_policy.selected_character != Some(plan.protagonist)
        {
            return Err(invalid());
        }
        self.floor_activation = Some(ActivationState {
            plan: Shared::new(plan),
            scopes: Shared::default(),
            members: Shared::default(),
            floors: Shared::default(),
            order: Shared::default(),
            decision_count: 0,
        });
        Ok(())
    }
    /// Only immutable bounded input is registered. Repeating identical input is
    /// harmless; disagreement is rejected instead of depending on worker order.
    pub fn register_activation_scope(&mut self, scope: PreparedScope) -> Result<(), GameError> {
        let Some(state) = &mut self.floor_activation else {
            return Err(invalid());
        };
        if !scope.valid() || !state.plan.profiles.contains_key(&scope.profile) {
            return Err(invalid());
        }
        if let Some(previous) = state.scopes.get(&scope.scope) {
            return if **previous == scope {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        if state.scopes.len() >= MAX_SCOPES {
            return Err(GameError::IdentityExhausted);
        }
        if scope
            .members
            .iter()
            .any(|member| state.members.contains_key(member))
        {
            return Err(invalid());
        }
        for member in &scope.members {
            state.members.insert(*member, scope.scope.clone());
        }
        state.scopes.insert(scope.scope.clone(), Shared::new(scope));
        Ok(())
    }
    pub fn population_configuration_matches(
        &self,
        plan: Option<&PopulationPlan>,
        capabilities: &ScenarioCapabilities,
        selected_character: Option<ActorId>,
    ) -> bool {
        &self.scenario_policy.capabilities == capabilities
            && self.scenario_policy.selected_character == selected_character
            && self.floor_activation.as_ref().map(|state| &*state.plan) == plan
    }
    pub fn activation_scopes(&self) -> impl Iterator<Item = &PreparedScope> {
        self.floor_activation
            .iter()
            .flat_map(|state| state.scopes.values().map(|scope| &**scope))
    }
    pub fn activation_count(&self) -> usize {
        self.floor_activation.as_ref().map_or(0, |s| s.order.len())
    }
    pub fn activations_since(
        &self,
        previous_count: usize,
    ) -> Result<Vec<FloorActivationRecord>, GameError> {
        let Some(state) = &self.floor_activation else {
            return if previous_count == 0 {
                Ok(vec![])
            } else {
                Err(invalid())
            };
        };
        let suffix = state.order.get(previous_count..).ok_or_else(invalid)?;
        Ok(suffix
            .iter()
            .map(|key| (*state.floors[key]).clone())
            .collect())
    }
    pub fn activate_initial_floor(&mut self) -> Result<(), GameError> {
        let Some(state) = &self.floor_activation else {
            return Ok(());
        };
        let id = state.plan.protagonist;
        let to = self
            .actors
            .get(&id)
            .ok_or(GameError::UnknownActor)?
            .location;
        self.activate_entry(id, to, EntryCause::Initial)
    }
    pub(crate) fn population_participates_in_entry(&self, id: ActorId, to: Location) -> bool {
        self.floor_activation.as_ref().is_some_and(|state| {
            id == state.plan.protagonist && state.members.contains_key(&to.region)
        })
    }
    pub(crate) fn accepted_region_entry(
        &mut self,
        id: ActorId,
        from: Location,
        to: Location,
        cause: EntryCause,
    ) -> Result<(), GameError> {
        if from.region != to.region {
            let participates = self.population_participates_in_entry(id, to);
            self.activate_entry(id, to, cause)?;
            // Revisits settle at the same exact entry boundary as first visits.
            // Ordinary games without population retain existing continuation.
            if participates && self.host_region_continuation {
                self.population_entry_boundary = Some(self.tick);
            }
        }
        Ok(())
    }
    pub fn population_entry_pending(&self) -> bool {
        self.population_entry_boundary.is_some()
    }
    /// The host calls this only after successful no-time-advance horizon
    /// settlement. A subsequent entry may request another boundary; it is still
    /// the same unpublished candidate and the same ordinary scheduler.
    pub fn complete_region_transition(
        &mut self,
        report: &crate::TransitionReport,
    ) -> Result<(), GameError> {
        let pending = self.population_entry_boundary.take();
        if pending.is_some_and(|tick| tick != self.tick) {
            return Err(invalid());
        }
        if pending.is_some() || !report.frozen.is_empty() {
            self.host_region_continuation = true;
            let result = self.advance_to_next_decision();
            self.host_region_continuation = false;
            result?;
        }
        Ok(())
    }
    /// Extend the normal load closure only. Entries into any reachable scope
    /// then have every bounded member ready before simulation can continue.
    pub(crate) fn activation_loaded_closure(&self, loaded: &mut BTreeSet<RegionId>) {
        let Some(state) = &self.floor_activation else {
            return;
        };
        let scopes: BTreeSet<_> = loaded
            .iter()
            .filter_map(|id| state.members.get(id))
            .filter(|scope| !state.floors.contains_key(*scope))
            .cloned()
            .collect();
        for scope in scopes {
            loaded.extend(state.scopes[&scope].members.iter().copied());
        }
    }
    fn activate_entry(
        &mut self,
        id: ActorId,
        to: Location,
        cause: EntryCause,
    ) -> Result<(), GameError> {
        let Some(state) = &self.floor_activation else {
            return Ok(());
        };
        if id != state.plan.protagonist {
            return Ok(());
        }
        let Some(scope_id) = state.members.get(&to.region) else {
            return Ok(());
        };
        if state.floors.contains_key(scope_id) {
            return Ok(());
        }
        let scope = state.scopes[scope_id].clone();
        let profile = state.plan.profiles[&scope.profile].clone();
        if scope
            .members
            .iter()
            .any(|id| !self.world.loaded_regions().any(|loaded| loaded == *id))
        {
            return Err(GameError::InvalidLocation);
        }
        let budget: usize = profile
            .stages
            .iter()
            .map(|s| 1 + usize::from(s.count[1]))
            .sum();
        if state
            .decision_count
            .checked_add(budget)
            .is_none_or(|n| n > MAX_RETAINED_DECISIONS)
        {
            return Err(GameError::IdentityExhausted);
        }
        let ordinal = state.order.len() as u64;
        let mut decisions = Vec::new();
        let mut newborn = Vec::new();
        for stage in &profile.stages {
            let result = if scope.depth < stage.minimum_depth {
                StageResult::Skipped {
                    reason: SkipReason::BelowMinimumDepth,
                }
            } else {
                let mut count_rng = stream(
                    self.seed,
                    &scope.scope,
                    &scope.profile,
                    &profile,
                    stage,
                    0,
                    "count",
                );
                let requested = stage.count[0]
                    + index(
                        &mut count_rng,
                        usize::from(stage.count[1] - stage.count[0]) + 1,
                    ) as u16;
                let mut attempts = Vec::new();
                for attempt in 0..requested {
                    let mut choice_rng = stream(
                        self.seed,
                        &scope.scope,
                        &scope.profile,
                        &profile,
                        stage,
                        u64::from(attempt),
                        "content",
                    );
                    let mut placement_rng = stream(
                        self.seed,
                        &scope.scope,
                        &scope.profile,
                        &profile,
                        stage,
                        u64::from(attempt),
                        "placement",
                    );
                    let mut cells = scope.cells.clone();
                    for i in (1..cells.len()).rev() {
                        let j = index(&mut placement_rng, i + 1);
                        cells.swap(i, j);
                    }
                    let occupied = |game: &Game, at: Location| {
                        scope.protected.contains(&at)
                            || !game.world.walkable(at)
                            || game
                                .actors
                                .at(&game.world, at)
                                .keys()
                                .any(|other| *other == id || game.actors[other].alive())
                    };
                    let decision = match &stage.population {
                        PopulationContent::Items { choices } => {
                            let choice = &choices[index(&mut choice_rng, choices.len())];
                            let at = cells.into_iter().find(|at| {
                                !occupied(self, *at)
                                    && self.items.at(ItemLocation::Ground(*at)).next().is_none()
                            });
                            if let Some(location) = at {
                                let item = self.spawn_item_stack(
                                    location,
                                    None,
                                    choice.quantity,
                                    choice.spec.clone(),
                                )?;
                                PlacementDecision::Placed {
                                    content: choice.content.clone(),
                                    location,
                                    entity: PopulationEntity::Item { id: item },
                                }
                            } else {
                                PlacementDecision::Skipped {
                                    content: choice.content.clone(),
                                    reason: SkipReason::NoEligibleCell,
                                }
                            }
                        }
                        PopulationContent::Actors {
                            choices,
                            avoid_protagonist_sight,
                        } => {
                            let choice = &choices[index(&mut choice_rng, choices.len())];
                            let body = choice.actor.body.clone().unwrap_or_default();
                            let seen: BTreeSet<_> = if *avoid_protagonist_sight {
                                self.scene(id)?
                                    .into_iter()
                                    .map(|cell| cell.location)
                                    .collect()
                            } else {
                                BTreeSet::new()
                            };
                            let at = cells.into_iter().find(|at| {
                                !occupied(self, *at)
                                    && !seen.contains(at)
                                    && self.body_fits(ActorId(0), *at, 0, &body)
                                    && self.body_cells(*at, 0, &body).is_some_and(|cells| {
                                        cells.iter().all(|(at, _)| {
                                            scope.members.contains(&at.region)
                                                && !occupied(self, *at)
                                                && !seen.contains(at)
                                        })
                                    })
                            });
                            if let Some(location) = at {
                                let actor = self.spawn_actor(
                                    location,
                                    NonZeroU64::new(choice.turn_ticks).ok_or_else(invalid)?,
                                )?;
                                let seed = stream(
                                    self.seed,
                                    &scope.scope,
                                    &scope.profile,
                                    &profile,
                                    stage,
                                    u64::from(attempt),
                                    "build",
                                );
                                self.configure_prepared_actor(actor, &choice.actor, seed)
                                    .map_err(|_| invalid())?;
                                newborn.push(actor);
                                PlacementDecision::Placed {
                                    content: choice.content.clone(),
                                    location,
                                    entity: PopulationEntity::Actor { id: actor },
                                }
                            } else {
                                PlacementDecision::Skipped {
                                    content: choice.content.clone(),
                                    reason: SkipReason::NoEligibleCell,
                                }
                            }
                        }
                    };
                    attempts.push(decision);
                }
                StageResult::Evaluated {
                    requested,
                    attempts,
                }
            };
            decisions.push(StageDecision {
                id: stage.id.clone(),
                version: stage.version,
                result,
            });
        }
        let count = decisions
            .iter()
            .map(|d| {
                1 + match &d.result {
                    StageResult::Skipped { .. } => 0,
                    StageResult::Evaluated { attempts, .. } => attempts.len(),
                }
            })
            .sum::<usize>();
        let record = FloorActivationRecord {
            scope: scope.scope.clone(),
            entrant: id,
            tick: self.tick,
            ordinal,
            cause,
            profile: scope.profile.clone(),
            profile_version: profile.version,
            decisions,
        };
        let state = self
            .floor_activation
            .as_mut()
            .expect("configured population");
        state.decision_count += count;
        state.order.push(scope.scope.clone());
        state
            .floors
            .insert(scope.scope.clone(), Shared::new(record));
        for actor in newborn {
            let scene = self.scene(actor)?;
            self.refresh_navigation_scene(actor, &scene);
        }
        Ok(())
    }
}

impl Game {
    /// Validate historical origin evidence against the pinned definitions, not
    /// present ownership. Abandoned branches may name scopes not in this Game
    /// and may have later ticks/IDs than the restored boundary being audited.
    pub fn validate_activation_record(
        &self,
        record: &FloorActivationRecord,
    ) -> Result<(), GameError> {
        let state = self.floor_activation.as_ref().ok_or_else(invalid)?;
        let profile = state
            .plan
            .profiles
            .get(&record.profile)
            .ok_or_else(invalid)?;
        let depth: u32 = record
            .scope
            .rsplit_once('/')
            .and_then(|(_, depth)| depth.parse().ok())
            .filter(|depth| *depth > 0)
            .ok_or_else(invalid)?;
        if !label(&record.scope)
            || record.entrant != state.plan.protagonist
            || record.profile_version != profile.version
            || record.ordinal >= MAX_SCOPES as u64
            || record.decisions.len() != profile.stages.len()
        {
            return Err(invalid());
        }
        for (decision, stage) in record.decisions.iter().zip(&profile.stages) {
            if decision.id != stage.id || decision.version != stage.version {
                return Err(invalid());
            }
            match &decision.result {
                StageResult::Skipped {
                    reason: SkipReason::BelowMinimumDepth,
                } if depth < stage.minimum_depth => {}
                StageResult::Evaluated {
                    requested,
                    attempts,
                } if depth >= stage.minimum_depth
                    && (stage.count[0]..=stage.count[1]).contains(requested)
                    && attempts.len() == usize::from(*requested) =>
                {
                    let mut count_rng = stream(
                        self.seed,
                        &record.scope,
                        &record.profile,
                        profile,
                        stage,
                        0,
                        "count",
                    );
                    let expected = stage.count[0]
                        + index(
                            &mut count_rng,
                            usize::from(stage.count[1] - stage.count[0]) + 1,
                        ) as u16;
                    if *requested != expected {
                        return Err(invalid());
                    }
                    for (ordinal, attempt) in attempts.iter().enumerate() {
                        let content = match attempt {
                            PlacementDecision::Placed {
                                content,
                                location,
                                entity,
                            } => {
                                if location.region.0 == 0
                                    || match (entity, &stage.population) {
                                        (
                                            PopulationEntity::Actor { id },
                                            PopulationContent::Actors { .. },
                                        ) => id.0 == 0 || id.0 == u64::MAX,
                                        (
                                            PopulationEntity::Item { id },
                                            PopulationContent::Items { .. },
                                        ) => id.0 == 0 || id.0 == u64::MAX,
                                        _ => true,
                                    }
                                {
                                    return Err(invalid());
                                }
                                content
                            }
                            PlacementDecision::Skipped {
                                content,
                                reason: SkipReason::NoEligibleCell,
                            } => content,
                            _ => return Err(invalid()),
                        };
                        let mut choice_rng = stream(
                            self.seed,
                            &record.scope,
                            &record.profile,
                            profile,
                            stage,
                            ordinal as u64,
                            "content",
                        );
                        let expected = match &stage.population {
                            PopulationContent::Actors { choices, .. } => {
                                &choices[index(&mut choice_rng, choices.len())].content
                            }
                            PopulationContent::Items { choices } => {
                                &choices[index(&mut choice_rng, choices.len())].content
                            }
                        };
                        if expected != content {
                            return Err(invalid());
                        }
                    }
                }
                _ => return Err(invalid()),
            }
        }
        Ok(())
    }
    /// Check high-water identities in an audit context that retains all branch
    /// allocation ceilings. Historical origin does not require live ownership.
    pub fn validate_activation_record_identity_bounds(
        &self,
        record: &FloorActivationRecord,
    ) -> Result<(), GameError> {
        let mut actors = BTreeSet::new();
        let mut items = BTreeSet::new();
        for decision in &record.decisions {
            if let StageResult::Evaluated { attempts, .. } = &decision.result {
                for attempt in attempts {
                    if let PlacementDecision::Placed { entity, .. } = attempt {
                        let valid = match entity {
                            PopulationEntity::Actor { id } => {
                                id.0 > 0 && id.0 < self.next_actor_id && actors.insert(*id)
                            }
                            PopulationEntity::Item { id } => {
                                id.0 > 0 && id.0 < self.next_item_id && items.insert(*id)
                            }
                        };
                        if !valid {
                            return Err(invalid());
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn activation_state_valid(&self) -> bool {
        if self
            .scenario_policy
            .selected_character
            .is_some_and(|id| id.0 == 0)
        {
            return false;
        }
        let Some(state) = &self.floor_activation else {
            return true;
        };
        if !state.plan.valid(self)
            || self.scenario_policy.selected_character != Some(state.plan.protagonist)
            || state.scopes.len() > MAX_SCOPES
            || state.floors.len() != state.order.len()
            || state.floors.len() > MAX_SCOPES
            || state.decision_count > MAX_RETAINED_DECISIONS
        {
            return false;
        }
        let mut members = BTreeMap::new();
        for (key, scope) in state.scopes.iter() {
            if key != &scope.scope
                || !scope.valid()
                || !state.plan.profiles.contains_key(&scope.profile)
            {
                return false;
            }
            for member in &scope.members {
                if members.insert(*member, key.clone()).is_some() {
                    return false;
                }
            }
        }
        if members != *state.members {
            return false;
        }
        let mut seen = BTreeSet::new();
        let mut actor_origins = BTreeSet::new();
        let mut item_origins = BTreeSet::new();
        let mut decisions = 0usize;
        for (ordinal, scope_id) in state.order.iter().enumerate() {
            if !seen.insert(scope_id) {
                return false;
            }
            let Some(record) = state.floors.get(scope_id) else {
                return false;
            };
            let Some(scope) = state.scopes.get(scope_id) else {
                return false;
            };
            if record.scope != *scope_id
                || record.profile != scope.profile
                || record.tick > self.tick
                || record.ordinal != ordinal as u64
                || self.validate_activation_record(record).is_err()
                || self
                    .validate_activation_record_identity_bounds(record)
                    .is_err()
            {
                return false;
            }
            for decision in &record.decisions {
                decisions += 1;
                if let StageResult::Evaluated { attempts, .. } = &decision.result {
                    decisions += attempts.len();
                    for attempt in attempts {
                        if let PlacementDecision::Placed {
                            location, entity, ..
                        } = attempt
                        {
                            let unique = match entity {
                                PopulationEntity::Actor { id } => actor_origins.insert(*id),
                                PopulationEntity::Item { id } => item_origins.insert(*id),
                            };
                            if !unique {
                                return false;
                            }
                            if !scope.members.contains(&location.region)
                                || scope.cells.binary_search(location).is_err()
                                || scope.protected.contains(location)
                            {
                                return false;
                            }
                        }
                    }
                }
            }
        }
        decisions == state.decision_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, BodySpec, Direction, MemoryRecords, RegionTransition};
    use tor_world::Position;

    include!("floor_activation_transaction_tests.rs");
    include!("floor_activation_checkpoint_tests.rs");

    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }
    fn marker() -> ItemSpec {
        let mut spec = ItemSpec::ordinary("marker".into());
        spec.stackable = true;
        spec
    }
    fn game() -> Game {
        let content =
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap();
        let mut game = Game::region_corridor(42, 2, content);
        game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
            .unwrap();
        game
    }
    fn item_stage(minimum_depth: u32) -> PopulationStage {
        PopulationStage {
            id: "stock".into(),
            version: 1,
            minimum_depth,
            count: [1, 1],
            population: PopulationContent::Items {
                choices: vec![PreparedItemChoice {
                    content: "marker".into(),
                    quantity: 1,
                    spec: marker(),
                }],
            },
        }
    }
    fn scope(depth: u32, cells: Vec<Location>) -> PreparedScope {
        PreparedScope {
            scope: format!("dungeon/{depth}"),
            depth,
            profile: "baseline".into(),
            members: BTreeSet::from([RegionId(u64::from(depth))]),
            cells,
            protected: BTreeSet::new(),
        }
    }
    fn plan(stages: Vec<PopulationStage>) -> PopulationPlan {
        PopulationPlan {
            protagonist: ActorId(1),
            profiles: BTreeMap::from([(
                "baseline".into(),
                PopulationProfile { version: 1, stages },
            )]),
        }
    }

    #[test]
    fn prepared_scopes_allocate_nothing_and_initial_receipt_is_once_only() {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut later = item_stage(3);
        later.id = "later".into();
        game.configure_population(plan(vec![item_stage(1), later]))
            .unwrap();
        let before = (
            game.next_actor_id,
            game.next_item_id,
            game.rules_rng,
            game.tick,
        );
        let prepared = scope(1, vec![at(1, 2, 1)]);
        game.register_activation_scope(prepared.clone()).unwrap();
        game.register_activation_scope(prepared).unwrap();
        assert_eq!(
            (
                game.next_actor_id,
                game.next_item_id,
                game.rules_rng,
                game.tick
            ),
            before
        );
        assert_eq!(game.activation_count(), 0);
        game.activate_initial_floor().unwrap();
        let records = game.activations_since(0).unwrap();
        assert_eq!(records.len(), 1);
        assert!(matches!(
            records[0].decisions[0].result,
            StageResult::Evaluated { requested: 1, .. }
        ));
        assert_eq!(
            records[0].decisions[1].result,
            StageResult::Skipped {
                reason: SkipReason::BelowMinimumDepth
            }
        );
        let after = game.clone();
        game.activate_initial_floor().unwrap();
        assert_eq!(game, after);
        assert_eq!(game.rules_rng, before.2);
    }

    #[test]
    fn merged_origin_id_remains_valid_historical_evidence() {
        let mut game = game();
        let owner = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_population(plan(vec![item_stage(1)]))
            .unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let record = game.activations_since(0).unwrap().remove(0);
        let carried = game
            .spawn_item_stack(at(1, 1, 1), Some(owner), 1, marker())
            .unwrap();
        let origin = ItemId(1);
        // Ordinary pickup requires occupying the ground stack's exact cell.
        game.act(owner, Action::Move(Direction::East)).unwrap();
        assert_eq!(game.actors[&owner].location, at(1, 2, 1));
        game.act(
            owner,
            Action::Take {
                item: origin,
                quantity: None,
            },
        )
        .unwrap();
        assert!(
            !game.items.contains_key(&origin),
            "ordinary pickup must merge and retire the origin identity"
        );
        assert_eq!(game.items[&carried].quantity, 2);
        game.validate_activation_record(&record).unwrap();
        game.validate_activation_record_identity_bounds(&record)
            .unwrap();
        assert!(game.activation_state_valid());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
        assert_eq!(restored, game);
    }

    #[test]
    fn aggregate_attempt_limit_rejects_before_any_mutation() {
        let mut game = game();
        let before = game.clone();
        let mut a = item_stage(1);
        a.count = [MAX_ATTEMPTS, MAX_ATTEMPTS];
        let mut b = item_stage(1);
        b.id = "extra".into();
        assert!(game.configure_population(plan(vec![a, b])).is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn preparation_order_does_not_change_choices_and_clones_share_immutable_inputs() {
        let mut forward = game();
        forward
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        forward
            .configure_population(plan(vec![item_stage(1)]))
            .unwrap();
        let mut reverse = forward.clone();
        let first = scope(1, vec![at(1, 2, 1), at(1, 3, 1)]);
        let second = scope(2, vec![at(2, 2, 1), at(2, 3, 1)]);
        forward.register_activation_scope(first.clone()).unwrap();
        forward.register_activation_scope(second.clone()).unwrap();
        reverse.register_activation_scope(second).unwrap();
        reverse.register_activation_scope(first).unwrap();
        forward.activate_initial_floor().unwrap();
        reverse.activate_initial_floor().unwrap();
        forward.teleport(ActorId(1), at(2, 0, 1)).unwrap();
        reverse.teleport(ActorId(1), at(2, 0, 1)).unwrap();
        assert_eq!(forward.activations_since(0), reverse.activations_since(0));
        assert_eq!(forward, reverse);
        let cloned = forward.clone();
        let original = forward.floor_activation.as_ref().unwrap();
        let copy = cloned.floor_activation.as_ref().unwrap();
        assert!(original.plan.shares_storage(&copy.plan));
        assert!(original.scopes.shares_storage(&copy.scopes));
        assert!(original.floors.shares_storage(&copy.floors));
        assert!(original.order.shares_storage(&copy.order));
    }

    #[test]
    fn persisted_population_rejects_duplicate_keys_positional_records_and_missing_fields() {
        let encoded = serde_json::to_string(&plan(vec![item_stage(1)])).unwrap();
        let decoded: PopulationPlan = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, plan(vec![item_stage(1)]));
        let profile = serde_json::to_string(&decoded.profiles["baseline"]).unwrap();
        let duplicate = format!(
            r#"{{"protagonist":1,"profiles":{{"baseline":{profile},"baseline":{profile}}}}}"#
        );
        assert!(serde_json::from_str::<PopulationPlan>(&duplicate).is_err());
        assert!(serde_json::from_str::<PopulationPlan>(r#"[1,{}]"#).is_err());
        let mut scope_value = serde_json::to_value(scope(1, vec![at(1, 2, 1)])).unwrap();
        scope_value["cells"][0]["position"] = serde_json::json!([2, 1, 0]);
        assert!(serde_json::from_value::<PreparedScope>(scope_value).is_err());
        assert!(serde_json::from_str::<PreparedActor>(r#"{"known_identities":[]}"#).is_err());
    }

    #[test]
    fn receipt_identity_bounds_reject_forged_origins_without_requiring_live_ownership() {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_population(plan(vec![item_stage(1)]))
            .unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let mut record = game.activations_since(0).unwrap().remove(0);
        if let StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result {
            if let PlacementDecision::Placed { entity, .. } = &mut attempts[0] {
                *entity = PopulationEntity::Item {
                    id: ItemId(game.next_item_id),
                };
            } else {
                panic!("fixture must place an item");
            }
        } else {
            panic!("fixture must evaluate a stage");
        }
        assert!(game
            .validate_activation_record_identity_bounds(&record)
            .is_err());
    }

    #[test]
    fn receipt_audit_rejects_another_valid_but_unselected_content_choice() {
        let mut game = game();
        game.spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut stock = item_stage(1);
        if let PopulationContent::Items { choices } = &mut stock.population {
            let mut alternate = choices[0].clone();
            alternate.content = "alternate".into();
            choices.push(alternate);
        }
        game.configure_population(plan(vec![stock])).unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 2, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let mut record = game.activations_since(0).unwrap().remove(0);
        game.validate_activation_record(&record).unwrap();
        if let StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result {
            if let PlacementDecision::Placed { content, .. } = &mut attempts[0] {
                *content = if content == "marker" {
                    "alternate"
                } else {
                    "marker"
                }
                .into();
            } else {
                panic!("fixture must place an item");
            }
        } else {
            panic!("fixture must evaluate a stage");
        }
        assert!(game.validate_activation_record(&record).is_err());
    }

    fn falling_entry() -> (Game, ActorId, ActorId, ItemId, MemoryRecords) {
        falling_entry_from(game())
    }

    fn falling_entry_from(mut game: Game) -> (Game, ActorId, ActorId, ItemId, MemoryRecords) {
        let protagonist = game
            .spawn_actor(at(1, 8, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let driver = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mover = game
            .spawn_actor(at(1, 2, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.actors.get_mut(&protagonist).unwrap().ready_at = 100;
        game.actors.get_mut(&mover).unwrap().ready_at = 100;
        game.set_actor_velocity(protagonist, [crate::physics::TERMINAL, 0, 0])
            .unwrap();
        game.set_actor_velocity(mover, [crate::physics::TERMINAL, 0, 0])
            .unwrap();
        let old_item = game
            .spawn_item_stack(at(1, 2, 0), None, 1, marker())
            .unwrap();
        game.items
            .edit(old_item, |item| {
                item.motion.velocity = [crate::physics::TERMINAL, 0, 0]
            })
            .unwrap();
        game.set_gravity(RegionId(2), [1, 0, 0]).unwrap();
        let actor = PreparedActor {
            build: None,
            nutrition: crate::nutrition::NutritionRequirement::Exempt,
            identity: None,
            anatomy: None,
            body: Some(BodySpec::default()),
            ai: None,
            known_identities: vec![],
            velocity: None,
            asset: None,
        };
        game.configure_population(plan(vec![
            item_stage(2),
            PopulationStage {
                id: "newborn".into(),
                version: 1,
                minimum_depth: 2,
                count: [1, 1],
                population: PopulationContent::Actors {
                    choices: vec![PreparedActorChoice {
                        content: "ordinary actor".into(),
                        turn_ticks: 100,
                        actor,
                    }],
                    avoid_protagonist_sight: false,
                },
            },
        ]))
        .unwrap();
        game.register_activation_scope(scope(1, vec![at(1, 4, 1)]))
            .unwrap();
        game.register_activation_scope(scope(2, vec![at(2, 3, 1), at(2, 4, 1)]))
            .unwrap();
        game.activate_initial_floor().unwrap();
        let mut records = MemoryRecords::default();
        game.transition_regions_counted_deferred(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
            },
            &mut records,
        )
        .unwrap();
        assert!(
            game.region_frozen(RegionId(2)),
            "destination must really be frozen before motion"
        );
        (game, driver, mover, old_item, records)
    }

    #[test]
    fn host_physics_boundary_finishes_old_tick_without_backdating_newborns() {
        let (mut game, driver, mover, item, mut records) = falling_entry();
        let result = game.act_for_host(driver, Action::Wait).unwrap();
        assert!(matches!(result, CandidateStep::AwaitingRegions(_)));
        let entered = game.activations_since(1).unwrap();
        assert_eq!(entered.len(), 1);
        let tick = entered[0].tick;
        assert_eq!(tick, 32);
        assert_eq!(game.tick, tick);
        assert_eq!(
            game.actors[&mover].location,
            at(1, 6, 2),
            "higher-ID mover must finish tick T"
        );
        assert_eq!(
            game.items[&item].location,
            ItemLocation::Ground(at(1, 6, 0)),
            "old loose item must finish tick T"
        );
        let newborn = ActorId(4);
        assert_eq!(game.actors[&newborn].active_since, tick);
        assert_eq!(game.actors[&newborn].ready_at, tick);
        assert!(game.actor_frozen(newborn));
        let born_item = ItemId(2);
        assert_eq!(
            game.items[&born_item].motion.velocity, [0; 3],
            "new item gets no retroactive birth-tick gravity"
        );
        let (_, report, _) = game
            .transition_regions_counted_deferred(
                &RegionTransition {
                    active: BTreeSet::from([RegionId(2)]),
                    loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                },
                &mut records,
            )
            .unwrap();
        game.complete_region_transition(&report).unwrap();
        assert!(!game.population_entry_pending());
        assert_eq!(game.tick, tick);
        assert_eq!(game.next_actor(), Some(newborn));
        assert_eq!(game.items[&born_item].motion.velocity, [0; 3]);
    }

    #[test]
    fn lethal_post_entry_slide_finishes_the_original_host_physics_tick() {
        for destination_active in [false, true] {
            let world = tor_world::World::new(
                (1..=2)
                    .map(|id| tor_world::Region {
                        id: RegionId(id),
                        name: format!("wide corridor {id}"),
                        bounds: tor_world::Extent::new(12, 9, 1).unwrap(),
                    })
                    .collect(),
                vec![],
            )
            .unwrap();
            let mut game = Game::new(world, 42, crate::test_creatures::content());
            game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
                .unwrap();
            for (from, direction, to) in [
                (at(1, 11, 0), Direction::East, at(2, 0, 0)),
                (at(2, 0, 0), Direction::West, at(1, 11, 0)),
            ] {
                game.connect_area(
                    tor_world::Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                    9,
                    1,
                )
                .unwrap();
            }
            let (mut game, driver, mover, item, mut records) = falling_entry_from(game);
            let protagonist = ActorId(1);
            crate::test_creatures::configure(
                &mut game,
                protagonist,
                "player",
                crate::test_creatures::species(),
            );
            crate::test_creatures::injure(&mut game, protagonist, 29).unwrap();
            game.configure_run(
                protagonist,
                BTreeSet::from([protagonist]),
                None,
                BTreeMap::new(),
            )
            .unwrap();
            game.set_actor_velocity(protagonist, [4096, 4096, 0])
                .unwrap();
            game.set_wall(at(2, 0, 5), true).unwrap();
            assert!(game.region_frozen(RegionId(2)));
            if destination_active {
                game.transition_regions_counted_deferred(
                    &RegionTransition {
                        active: BTreeSet::from([RegionId(1), RegionId(2)]),
                        loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                    },
                    &mut records,
                )
                .unwrap();
                assert!(!game.region_frozen(RegionId(2)));
            }
            assert_eq!(game.health(protagonist), Some((1, 30)));
            let step = game.act_for_host(driver, Action::Wait).unwrap();
            assert!(matches!(step, CandidateStep::AwaitingRegions(_)));
            let entry = game.activations_since(1).unwrap();
            assert_eq!(entry.len(), 1);
            assert_eq!((entry[0].tick, game.tick), (64, 64));
            assert_eq!(game.actors[&protagonist].location, at(2, 0, 4));
            assert!(!game.alive(protagonist));
            assert!(
                game.run_outcome().terminal,
                "death must exercise the terminal early-return path"
            );
            assert_eq!(
                game.actors[&mover].location,
                at(1, 10, 2),
                "first red: a terminal post-entry collision must not omit the higher-ID original mover at tick T"
            );
            assert_eq!(
                game.items[&item].location,
                ItemLocation::Ground(at(1, 10, 0)),
                "terminal entry must finish the original loose item's tick T"
            );
            let newborn = ActorId(4);
            assert_eq!(game.actors[&newborn].active_since, 64);
            assert_eq!(game.actors[&newborn].ready_at, 64);
            assert_eq!(
                game.items[&ItemId(2)].motion.velocity,
                [0; 3],
                "new population items receive no retroactive birth-tick gravity"
            );
            let corpse = game
                .items
                .values()
                .find(|item| item.spec.class == crate::ItemClass::Corpse)
                .unwrap();
            assert_eq!(
                corpse.motion.displacement, [0; 3],
                "the new corpse also receives no birth-tick movement"
            );
            assert_eq!(corpse.motion.velocity, [4096, 0, 0]);
            let (_, report, _) = game
                .transition_regions_counted_deferred(
                    &RegionTransition {
                        active: BTreeSet::from([RegionId(1), RegionId(2)]),
                        loaded: BTreeSet::from([RegionId(1), RegionId(2)]),
                    },
                    &mut records,
                )
                .unwrap();
            game.complete_region_transition(&report).unwrap();
            assert!(!game.population_entry_pending());
            assert_eq!(game.tick, 64);
            assert_eq!(game.activation_count(), 2);
        }
    }

    #[test]
    fn ordinary_standalone_action_still_returns_a_settled_decision() {
        let (mut game, driver, _, _, _) = falling_entry();
        let result = game.act(driver, Action::Wait).unwrap();
        assert!(!game.population_entry_pending());
        assert_eq!(game.tick, 100);
        assert_eq!(result.next_tick, game.tick);
        assert_eq!(result.next_actor, game.next_actor());
        assert_eq!(game.next_actor(), Some(driver));
    }

    #[test]
    fn npc_abstract_stairs_are_role_gated_without_blocking_physical_portals() {
        let mut game = Game::region_corridor(
            42,
            2,
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap(),
        );
        game.configure_scenario_policy(
            ScenarioCapabilities {
                npc_stair_traversal: false,
            },
            Some(ActorId(1)),
        )
        .unwrap();
        let player = game
            .spawn_actor(at(1, 1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let npc = game
            .spawn_actor(at(1, 11, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.connect(
            tor_world::Passage {
                from: at(1, 11, 1),
                direction: Direction::Down,
                to: at(2, 5, 1),
            },
            0,
        )
        .unwrap();
        assert!(game.actor_translation(npc, Direction::Down).is_none());
        assert!(
            game.actor_translation(npc, Direction::East).is_some(),
            "ordinary physical cross-region movement remains legal"
        );
        game.refresh_navigation();
        assert!(!game.navigation[&npc]
            .edges
            .contains_key(&(at(1, 11, 1), Direction::Down)));
        // Moving the declared character to the same stair does not depend on AI
        // settings or any server controller assignment.
        game.teleport(npc, at(1, 8, 1)).unwrap();
        game.teleport(player, at(1, 11, 1)).unwrap();
        assert!(game.actor_translation(player, Direction::Down).is_some());
    }
}
