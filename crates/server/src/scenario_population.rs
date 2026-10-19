//! Bounded authoring adapters for once-only entry population and initial kits.
//! These declarations describe immutable content. Only simulation evaluates entry.
use super::*;
use tor_simulation::floor_activation as runtime;

const MAX_PROFILES: usize = runtime::MAX_PROFILES;
const MAX_STAGES: usize = runtime::MAX_STAGES;
const MAX_DEPTH_BANDS: usize = runtime::MAX_DEPTH_BANDS;
const MAX_CHOICES: usize = runtime::MAX_CHOICES;
const MAX_ATTEMPTS: u16 = runtime::MAX_ATTEMPTS;
const MAX_LOADOUT: usize = 64;
const MAX_CARRIED_ITEM_POOLS: usize = runtime::MAX_CARRIED_ITEM_POOLS;

/// Fixed prepared definitions and content for one population-plan compilation.
/// A label locates at most one candidate; every compiled recipe field must match.
struct PopulationBirthValidation<'a> {
    index: &'a PackageIndex,
    successful: BTreeMap<String, runtime::PreparedActorChoice>,
    #[cfg(test)]
    checks: usize,
}
impl<'a> PopulationBirthValidation<'a> {
    fn new(index: &'a PackageIndex) -> Self {
        Self {
            index,
            successful: BTreeMap::new(),
            #[cfg(test)]
            checks: 0,
        }
    }
    fn validate(&mut self, choice: &runtime::PreparedActorChoice) -> Result<(), Failure> {
        if self
            .successful
            .get(&choice.content)
            .is_some_and(|previous| previous.same_spawn_recipe(choice))
        {
            return Ok(());
        }
        #[cfg(test)]
        {
            self.checks += 1;
        }
        choice
            .actor
            .validate_birth_advancement(
                self.index.creature_catalog().content().clone(),
                choice.depth_growth,
                &choice.advancement,
            )
            .map_err(|error| {
                fail(format!(
                    "Invalid population depth-growth prototype: {error:?}"
                ))
            })?;
        // Never retain more than one band's capacity. Failures are not retained.
        if self.successful.len() < MAX_CHOICES || self.successful.contains_key(&choice.content) {
            self.successful
                .insert(choice.content.clone(), choice.clone());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioCapabilities {
    pub npc_stair_traversal: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Protagonist {
    Actor(u64),
    Selected(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FloorActivation {
    pub version: u32,
    pub protagonist: Protagonist,
    #[serde(default)]
    pub families: BTreeMap<String, String>,
    pub profiles: BTreeMap<String, PopulationProfile>,
    #[serde(default, deserialize_with = "read_carried_item_pools")]
    pub carried_item_pools: BTreeMap<String, CarriedItemPool>,
}

fn read_carried_item_pools<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, CarriedItemPool>, D::Error> {
    struct Pools;
    impl<'de> serde::de::Visitor<'de> for Pools {
        type Value = BTreeMap<String, CarriedItemPool>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("at most 32 uniquely named carried item pools")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut mapping: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if mapping
                .size_hint()
                .is_some_and(|size| size > MAX_CARRIED_ITEM_POOLS)
            {
                return Err(A::Error::custom("too many carried item pools"));
            }
            let mut pools = BTreeMap::new();
            while let Some(key) = mapping.next_key::<String>()? {
                if pools.len() >= MAX_CARRIED_ITEM_POOLS {
                    return Err(A::Error::custom("too many carried item pools"));
                }
                if pools.contains_key(&key) {
                    return Err(A::Error::custom("duplicate carried item pool"));
                }
                pools.insert(key, mapping.next_value()?);
            }
            Ok(pools)
        }
    }
    deserializer.deserialize_map(Pools)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationProfile {
    pub version: u32,
    pub stages: Vec<PopulationStage>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationDepth {
    Finite { minimum: u32, maximum: u32 },
    Open { minimum: u32 },
}

impl PopulationDepth {
    fn minimum(self) -> u32 {
        match self {
            Self::Finite { minimum, .. } | Self::Open { minimum } => minimum,
        }
    }
    fn maximum(self) -> Option<u32> {
        match self {
            Self::Finite { maximum, .. } => Some(maximum),
            Self::Open { .. } => None,
        }
    }
    fn compiled(self) -> runtime::DepthInterval {
        match self {
            Self::Finite { minimum, maximum } => {
                runtime::DepthInterval::Finite { minimum, maximum }
            }
            Self::Open { minimum } => runtime::DepthInterval::Open { minimum },
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationDepthGrowth {
    Fixed {},
    RacialAfter { depth: u32 },
}
impl Default for PopulationDepthGrowth {
    fn default() -> Self {
        Self::Fixed {}
    }
}
impl PopulationDepthGrowth {
    fn compiled(self) -> tor_simulation::prepared_spawn::DepthGrowth {
        match self {
            Self::Fixed {} => tor_simulation::prepared_spawn::DepthGrowth::Fixed {},
            Self::RacialAfter { depth } => {
                tor_simulation::prepared_spawn::DepthGrowth::RacialAfter { depth }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationBand<C = PopulationChoice> {
    pub depth: PopulationDepth,
    pub choices: Vec<C>,
}

pub type ActorPopulationBand = PopulationBand<ActorPopulationChoice>;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationChoice {
    pub archetype: String,
    pub weight: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorPopulationChoice {
    pub archetype: String,
    pub weight: u32,
    pub carried_loot: Option<CarriedLoot>,
    #[serde(
        default,
        deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, PopulationAdvancement, 256>"
    )]
    pub advancement: Vec<PopulationAdvancement>,
}

/// Birth-only choices for exact future racial ordinals, not a live HD formula.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationAdvancement {
    #[serde(deserialize_with = "read_birth_ordinal")]
    pub ordinal: u64,
    #[serde(
        default,
        deserialize_with = "crate::creature_authoring::bounded_strike_vec::<_, crate::creature_authoring::Purchase, 256>"
    )]
    pub purchases: Vec<crate::creature_authoring::Purchase>,
    pub attribute: Option<crate::creature_authoring::Attribute>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedItemPool {
    pub version: u32,
    pub choices: Vec<CarriedItemChoice>,
}

fn read_birth_ordinal<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    let ordinal = u64::deserialize(d)?;
    if !(1..=tor_simulation::limits::HIT_DICE as u64).contains(&ordinal) {
        return Err(serde::de::Error::custom(
            "invalid birth advancement ordinal",
        ));
    }
    Ok(ordinal)
}

/// Source convenience is separate from the canonical tagged compiled/save form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QuantityExpression {
    Fixed(u64),
    Expression(PopulationQuantityExpr),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationQuantityExpr {
    Fixed {
        quantity: u64,
    },
    Uniform {
        minimum: u64,
        maximum: u64,
    },
    DepthScaledUniform {
        minimum: u64,
        maximum_base: u64,
        per_depth: u64,
    },
}
impl PopulationQuantityExpr {
    pub fn compiled(self) -> tor_simulation::quantity::QuantityExpr {
        use tor_simulation::quantity::QuantityExpr;
        match self {
            Self::Fixed { quantity } => QuantityExpr::Fixed { quantity },
            Self::Uniform { minimum, maximum } => QuantityExpr::Uniform { minimum, maximum },
            Self::DepthScaledUniform {
                minimum,
                maximum_base,
                per_depth,
            } => QuantityExpr::DepthScaledUniform {
                minimum,
                maximum_base,
                per_depth,
            },
        }
    }
}
impl QuantityExpression {
    pub fn compiled(&self) -> tor_simulation::quantity::QuantityExpr {
        match *self {
            Self::Fixed(quantity) => tor_simulation::quantity::QuantityExpr::Fixed { quantity },
            Self::Expression(expression) => expression.compiled(),
        }
    }
    fn maximum(&self) -> Result<u64, Failure> {
        self.compiled()
            .resolve(Some(1))
            .map_err(|_| fail("Invalid quantity expression"))?;
        self.compiled()
            .resolve(Some(u32::MAX))
            .map(|range| range.maximum)
            .map_err(|_| fail("Quantity expression overflows its depth domain"))
    }
}
fn unit_population_quantity() -> QuantityExpression {
    QuantityExpression::Fixed(1)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedItemChoice {
    pub archetype: String,
    pub weight: u32,
    pub quantity: QuantityExpression,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarriedLoot {
    pub version: u32,
    pub pool: String,
    pub chance: CarryChance,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CarryChance {
    pub numerator: u32,
    pub denominator: u32,
}

impl CarriedLoot {
    fn compiled(&self) -> runtime::CarriedLoot {
        runtime::CarriedLoot {
            version: self.version,
            pool: self.pool.clone(),
            chance: runtime::CarryChance {
                numerator: self.chance.numerator,
                denominator: self.chance.denominator,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationStage {
    RoomItems {
        id: String,
        version: u32,
        bands: Vec<PopulationBand>,
        quantity: QuantityExpression,
        chance: CarryChance,
    },
    Items {
        id: String,
        version: u32,
        count: [u16; 2],
        bands: Vec<PopulationBand>,
        #[serde(default = "unit_population_quantity")]
        quantity: QuantityExpression,
    },
    Actors {
        id: String,
        version: u32,
        count: [u16; 2],
        bands: Vec<ActorPopulationBand>,
        ai: Option<String>,
        #[serde(default)]
        depth_growth: PopulationDepthGrowth,
        #[serde(default)]
        avoid_protagonist_sight: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharacterLoadout {
    pub archetype: String,
    #[serde(default = "unit_quantity")]
    pub quantity: u64,
    pub equipped_slot: Option<u16>,
}

impl PopulationStage {
    fn header(&self) -> (&str, u32, [u16; 2]) {
        match self {
            Self::RoomItems { id, version, .. } => (id, *version, [1, 1]),
            Self::Items {
                id, version, count, ..
            }
            | Self::Actors {
                id, version, count, ..
            } => (id, *version, *count),
        }
    }
}

impl Package {
    pub(super) fn check_population(&self) -> Result<(), Failure> {
        for character in &self.manifest.characters {
            require(
                character.loadout.len() <= MAX_LOADOUT,
                "Character loadout exceeds limit",
            )?;
            let mut slots = BTreeSet::new();
            for row in &character.loadout {
                require(
                    label(&row.archetype) && row.quantity > 0,
                    "Invalid character loadout",
                )?;
                self.author_archetype(&row.archetype)?;
                if let Some(slot) = row.equipped_slot {
                    require(
                        row.quantity == 1 && slots.insert(slot),
                        "Duplicate or stacked starting equipment",
                    )?;
                }
            }
        }
        let Some(plan) = &self.manifest.floor_activation else {
            return Ok(());
        };
        require(plan.version == 1, "Unsupported floor activation version")?;
        let protagonist = match &plan.protagonist {
            Protagonist::Actor(actor) => *actor,
            Protagonist::Selected(selector) => {
                require(
                    selector == "selected_character",
                    "Unknown protagonist selector",
                )?;
                self.selected
            }
        };
        require(
            protagonist == self.selected
                && self.manifest.characters.iter().any(|c| c.id == protagonist),
            "Activation protagonist must be the selected declared character",
        )?;
        require(
            !plan.profiles.is_empty()
                && plan.profiles.len() <= MAX_PROFILES
                && plan.families.len() <= MAX_REGIONS,
            "Floor activation definitions exceed limit",
        )?;
        for (family, profile) in &plan.families {
            require(
                self.manifest.procedural_worlds.contains_key(family)
                    && plan.profiles.contains_key(profile),
                "Unknown activation family or profile",
            )?;
        }
        require(
            plan.carried_item_pools.len() <= MAX_CARRIED_ITEM_POOLS,
            "Too many carried item pools",
        )?;
        for (name, pool) in &plan.carried_item_pools {
            require(
                label(name) && pool.version == 1,
                "Invalid carried item pool or unsupported version",
            )?;
            self.check_population_choices(
                pool.choices
                    .iter()
                    .map(|choice| (choice.archetype.as_str(), choice.weight)),
                pool.choices.len(),
            )?;
            for choice in &pool.choices {
                choice.quantity.maximum()?;
            }
        }
        for (name, profile) in &plan.profiles {
            require(
                label(name)
                    && profile.version == 1
                    && profile.stages.len() <= MAX_STAGES
                    && profile
                        .stages
                        .iter()
                        .map(|stage| u32::from(stage.header().2[1]))
                        .sum::<u32>()
                        <= u32::from(MAX_ATTEMPTS),
                "Invalid population profile or unsupported version",
            )?;
            let mut ids = BTreeSet::new();
            for stage in &profile.stages {
                let (id, version, count) = stage.header();
                require(
                    label(id)
                        && ids.insert(id)
                        && version == 1
                        && count[0] <= count[1]
                        && count[1] <= MAX_ATTEMPTS,
                    "Invalid population stage",
                )?;
                if let PopulationStage::Actors { depth_growth, .. } = stage {
                    require(
                        depth_growth.compiled().valid(),
                        "Invalid population depth growth",
                    )?;
                }
                match stage {
                    PopulationStage::Items {
                        bands, quantity, ..
                    }
                    | PopulationStage::RoomItems {
                        bands, quantity, ..
                    } => {
                        quantity.maximum()?;
                        if let PopulationStage::RoomItems { chance, .. } = stage {
                            require(
                                chance.denominator > 0 && chance.numerator <= chance.denominator,
                                "Invalid room population chance",
                            )?;
                        }
                        self.check_population_bands(bands, |choice| {
                            (&choice.archetype, choice.weight)
                        })?;
                    }
                    PopulationStage::Actors { bands, ai, .. } => {
                        self.check_population_bands(bands, |choice| {
                            (&choice.archetype, choice.weight)
                        })?;
                        if let Some(ai) = ai {
                            require(
                                self.manifest.ai_profiles.contains_key(ai),
                                "Unknown population AI profile",
                            )?;
                        }
                        for choice in bands.iter().flat_map(|band| &band.choices) {
                            if let Some(policy) = &choice.carried_loot {
                                require(
                                    policy.version == 1
                                        && label(&policy.pool)
                                        && plan.carried_item_pools.contains_key(&policy.pool)
                                        && policy.chance.denominator > 0
                                        && policy.chance.numerator <= policy.chance.denominator,
                                    "Invalid carried loot policy or unknown item pool",
                                )?;
                            }
                        }
                    }
                }
            }
        }
        // Validate every possible choice, including future depth bands and
        // zero-chance carried pools. Effective family themes are structural;
        // actual activation outcomes must never determine a palette.
        for (family, profile) in &plan.families {
            let definition = &self.manifest.procedural_worlds[family];
            let themes = definition
                .zone
                .as_ref()
                .and_then(|zone| self.manifest.zones.get(zone))
                .and_then(|zone| zone.themes.as_deref())
                .unwrap_or(&self.manifest.themes);
            let forecast = self.theme_assets(themes);
            let mut assets = BTreeSet::new();
            let item_asset = |key: &str| -> Result<Option<String>, Failure> {
                Ok(
                    compiler::item_asset(self.author_archetype(key)?, &self.manifest)
                        .map(str::to_owned),
                )
            };
            for stage in &plan.profiles[profile].stages {
                match stage {
                    PopulationStage::Items { bands, .. }
                    | PopulationStage::RoomItems { bands, .. } => {
                        for choice in bands.iter().flat_map(|band| &band.choices) {
                            assets.extend(item_asset(&choice.archetype)?);
                        }
                    }
                    PopulationStage::Actors { bands, .. } => {
                        for choice in bands.iter().flat_map(|band| &band.choices) {
                            assets.extend(self.author_archetype(&choice.archetype)?.asset.clone());
                            if let Some(policy) = &choice.carried_loot {
                                for item in &plan.carried_item_pools[&policy.pool].choices {
                                    assets.extend(item_asset(&item.archetype)?);
                                }
                            }
                        }
                    }
                }
            }
            for asset in assets {
                require(
                    forecast.contains(&asset),
                    format!(
                        "Activation family {family}: asset {asset} isn't among its themes' assets"
                    ),
                )?;
            }
        }
        Ok(())
    }

    fn check_population_bands<C>(
        &self,
        bands: &[PopulationBand<C>],
        choice_fields: impl Fn(&C) -> (&str, u32),
    ) -> Result<(), Failure> {
        require(
            !bands.is_empty() && bands.len() <= MAX_DEPTH_BANDS,
            "Invalid population stage depth bands",
        )?;
        require(
            bands.windows(2).all(|pair| {
                pair[0]
                    .depth
                    .maximum()
                    .is_some_and(|end| end < pair[1].depth.minimum())
            }),
            "Population depth bands must be ordered and non-overlapping",
        )?;
        for band in bands {
            require(
                band.depth.minimum() > 0
                    && band
                        .depth
                        .maximum()
                        .is_none_or(|end| end >= band.depth.minimum()),
                "Invalid population depth band",
            )?;
            self.check_population_choices(
                band.choices.iter().map(&choice_fields),
                band.choices.len(),
            )?;
        }
        Ok(())
    }

    fn check_population_choices<'a>(
        &self,
        choices: impl Iterator<Item = (&'a str, u32)>,
        count: usize,
    ) -> Result<(), Failure> {
        require(
            count > 0 && count <= MAX_CHOICES,
            "Invalid population choice count",
        )?;
        let mut unique = BTreeSet::new();
        let mut total = 0u32;
        for (archetype, weight) in choices {
            require(
                label(archetype) && weight > 0 && unique.insert(archetype),
                "Population choices require distinct content and positive weights",
            )?;
            total = total
                .checked_add(weight)
                .ok_or_else(|| fail("Population choice weights overflow"))?;
            self.author_archetype(archetype)?;
        }
        Ok(())
    }

    fn capabilities(&self) -> runtime::ScenarioCapabilities {
        runtime::ScenarioCapabilities {
            npc_stair_traversal: self
                .manifest
                .capabilities
                .as_ref()
                .is_none_or(|capabilities| capabilities.npc_stair_traversal),
        }
    }

    fn population_plan(
        &self,
        index: &PackageIndex,
    ) -> Result<Option<runtime::PopulationPlan>, Failure> {
        self.population_plan_with_births(&mut PopulationBirthValidation::new(index))
    }

    fn population_plan_with_births(
        &self,
        births: &mut PopulationBirthValidation<'_>,
    ) -> Result<Option<runtime::PopulationPlan>, Failure> {
        let index = births.index;
        let Some(plan) = &self.manifest.floor_activation else {
            return Ok(None);
        };
        let profiles = plan
            .profiles
            .iter()
            .map(|(name, profile)| {
                let stages = profile
                    .stages
                    .iter()
                    .map(|stage| {
                        let (id, version, count) = stage.header();
                        let population = match stage {
                            PopulationStage::Items { quantity, bands, .. }
                            | PopulationStage::RoomItems { quantity, bands, .. } => {
                                let bands = bands
                                    .iter()
                                    .map(|band| {
                                        Ok(runtime::DepthBand {
                                            depth: band.depth.compiled(),
                                            choices: band
                                                .choices
                                                .iter()
                                                .map(|choice| {
                                                    Ok(runtime::PreparedItemChoice {
                                                        content: choice.archetype.clone(),
                                                        weight: choice.weight,
                                                        quantity: quantity.compiled(),
                                                        spec: index.definitions.population_item(
                                                            &choice.archetype,
                                                            quantity.maximum()?,
                                                            index.seed,
                                                            &index.appearances,
                                                        )?,
                                                    })
                                                })
                                                .collect::<Result<_, Failure>>()?,
                                        })
                                    })
                                    .collect::<Result<_, Failure>>()?;
                                match stage {
                                    PopulationStage::RoomItems { chance, .. } => runtime::PopulationContent::RoomItems { bands, chance: runtime::CarryChance { numerator: chance.numerator, denominator: chance.denominator } },
                                    _ => runtime::PopulationContent::Items { bands },
                                }
                            },
                            PopulationStage::Actors {
                                ai,
                                bands,
                                depth_growth,
                                avoid_protagonist_sight,
                                ..
                            } => runtime::PopulationContent::Actors {
                                bands: bands
                                    .iter()
                                    .map(|band| {
                                        Ok(runtime::DepthBand {
                                            depth: band.depth.compiled(),
                                            choices: band
                                                .choices
                                                .iter()
                                                .map(|choice| {
                                                    let mut prepared =
                                                        index.definitions.population_actor(
                                                            &choice.archetype,
                                                            choice.weight,
                                                            ai.as_deref(),
                                                        )?;
                                                    prepared.carried_loot = choice
                                                        .carried_loot
                                                        .as_ref()
                                                        .map(CarriedLoot::compiled);
                                                    prepared.depth_growth = depth_growth.compiled();
                                                    prepared.advancement = choice.advancement.iter().map(|entry| {
                                                        Ok(tor_simulation::prepared_spawn::OrdinalAdvancement {
                                                            ordinal: tor_simulation::HdCount(entry.ordinal),
                                                            choices: crate::creature_authoring::compile_choices(&entry.purchases, entry.attribute.as_ref(), index.creature_catalog().content().rules())?,
                                                        })
                                                    }).collect::<Result<_, Failure>>()?;
                                                    births.validate(&prepared)?;
                                                    Ok(prepared)
                                                })
                                                .collect::<Result<_, Failure>>()?,
                                        })
                                    })
                                    .collect::<Result<_, Failure>>()?,
                                avoid_protagonist_sight: *avoid_protagonist_sight,
                            },
                        };
                        Ok(runtime::PopulationStage {
                            id: id.into(),
                            version,
                            count,
                            population,
                        })
                    })
                    .collect::<Result<_, Failure>>()?;
                Ok((
                    name.clone(),
                    runtime::PopulationProfile {
                        version: profile.version,
                        stages,
                    },
                ))
            })
            .collect::<Result<_, Failure>>()?;
        // Resolve all entries, including unused pools and policies with zero chance.
        // Preparation pins ordinary item specs without allocating any identity.
        let carried_item_pools = plan
            .carried_item_pools
            .iter()
            .map(|(name, pool)| {
                let choices = pool
                    .choices
                    .iter()
                    .map(|choice| {
                        Ok(runtime::PreparedItemChoice {
                            content: choice.archetype.clone(),
                            weight: choice.weight,
                            quantity: choice.quantity.compiled(),
                            spec: index.definitions.population_item(
                                &choice.archetype,
                                choice.quantity.maximum()?,
                                index.seed,
                                &index.appearances,
                            )?,
                        })
                    })
                    .collect::<Result<_, Failure>>()?;
                Ok((
                    name.clone(),
                    runtime::PreparedCarriedItemPool {
                        version: pool.version,
                        choices,
                    },
                ))
            })
            .collect::<Result<_, Failure>>()?;
        Ok(Some(runtime::PopulationPlan {
            protagonist: tor_simulation::ActorId(self.selected),
            profiles,
            carried_item_pools,
        }))
    }

    pub(super) fn configure_population(
        &self,
        game: &mut Game,
        index: &PackageIndex,
    ) -> Result<(), Failure> {
        game.configure_scenario_policy(
            self.capabilities(),
            Some(tor_simulation::ActorId(self.selected)),
        )
        .map_err(|error| fail(format!("Scenario capabilities: {error:?}")))?;
        if let Some(plan) = self.population_plan(index)? {
            game.configure_population(plan)
                .map_err(|error| fail(format!("Floor activation: {error:?}")))?;
        }
        if let Some(policy) = self.retrieval_policy(index)? {
            game.configure_unique_retrieval(policy)
                .map_err(|error| fail(format!("Unique retrieval: {error:?}")))?;
        }
        Ok(())
    }

    /// Compile once for a checkpoint recovery, then reuse across retained games.
    pub(crate) fn population_configuration(
        &self,
        seed: u64,
    ) -> Result<PopulationConfiguration<'_>, Failure> {
        let (plan, actor_floor, item_floor) = if self.manifest.floor_activation.is_some() {
            let index = self.index(seed)?;
            let character = self
                .manifest
                .characters
                .iter()
                .find(|character| character.id == self.selected)
                .ok_or_else(|| fail("Missing selected character"))?;
            // Base construction allocates one ordinary item identity per
            // selected loadout row before any initial population can run.
            let item_floor = index
                .ceilings
                .1
                .checked_add(character.loadout.len() as u64)
                .ok_or_else(|| fail("Population item identity floor overflow"))?;
            (self.population_plan(&index)?, index.ceilings.0, item_floor)
        } else {
            (None, 1, 1)
        };
        Ok(PopulationConfiguration {
            package: self,
            seed,
            plan,
            actor_floor,
            item_floor,
            capabilities: self.capabilities(),
            expected_scopes: Default::default(),
        })
    }

    /// Register only immutable geometry. This is equally safe in a preload worker.
    pub(super) fn activation_scope(
        &self,
        identity: &str,
        depth: u32,
        regions: &[&RegionDef],
        rooms: &[runtime::PreparedRoomGroup],
    ) -> Result<Option<runtime::PreparedScope>, Failure> {
        let Some(plan) = &self.manifest.floor_activation else {
            return Ok(None);
        };
        let profile = identity
            .rsplit_once('/')
            .and_then(|(family, _)| plan.families.get(family));
        let Some(profile) = profile else {
            return Ok(None);
        };
        let mut cells = BTreeSet::new();
        let mut protected = BTreeSet::new();
        for region in regions {
            // Generated RegionDef is the immutable terrain input: openings are
            // applied after walls by add_geometry. No mutable entity is read.
            let mut walls: BTreeSet<_> = region.walls.iter().copied().collect();
            for at in &region.openings {
                walls.remove(at);
            }
            for x in 0..region.size[0] {
                for y in 0..region.size[1] {
                    let at = loc(region.id, [x, y, 0]);
                    if !walls.contains(&[x, y, 0]) {
                        cells.insert(at);
                    }
                }
            }
            protected.extend(region.anchors.values().map(|at| loc(region.id, *at)));
        }
        Ok(Some(runtime::PreparedScope {
            scope: identity.into(),
            depth,
            profile: profile.clone(),
            members: regions.iter().map(|region| RegionId(region.id)).collect(),
            cells: cells.into_iter().collect(),
            protected,
            rooms: rooms.to_vec(),
        }))
    }

    /// Called once by original/base reconstruction after the starting scope is
    /// loaded, and before scheduling or the first player observation.
    pub(crate) fn initialize_selected_character(&self, game: &mut Game) -> Result<(), Failure> {
        let has_loadout = self
            .manifest
            .characters
            .iter()
            .find(|character| character.id == self.selected)
            .is_some_and(|character| !character.loadout.is_empty());
        if !has_loadout {
            return game.activate_initial_floor().map_err(|error| {
                crate::Failure::simulation_failure(error)
                    .unwrap_or_else(|| fail(format!("Initial floor activation: {error:?}")))
            });
        }
        let index = self.index(game.seed())?;
        self.initialize_selected_character_with_index(game, &index)
    }

    pub(super) fn initialize_selected_character_with_index(
        &self,
        game: &mut Game,
        index: &PackageIndex,
    ) -> Result<(), Failure> {
        let character = self
            .manifest
            .characters
            .iter()
            .find(|character| character.id == self.selected)
            .ok_or_else(|| fail("Missing selected character"))?;
        let actor = tor_simulation::ActorId(self.selected);
        // Empty ordinary packages keep their existing observation work unchanged.
        // A carried item uses its owner's exact admitted initial location.
        let at = if character.loadout.is_empty() {
            None
        } else {
            Some(
                game.observe(actor)
                    .map_err(|_| fail("Selected character is not loaded"))?
                    .location,
            )
        };
        for row in &character.loadout {
            let spec = index.definitions.population_item(
                &row.archetype,
                row.quantity,
                game.seed(),
                &index.appearances,
            )?;
            let item = game
                .spawn_item_stack(
                    at.expect("loadout location"),
                    Some(actor),
                    row.quantity,
                    spec,
                )
                .map_err(|error| fail(format!("Character loadout: {error:?}")))?;
            if let Some(slot) = row.equipped_slot {
                game.equip_authored(actor, item, tor_simulation::EquipmentSlotId(slot))
                    .map_err(|_| {
                        fail("Starting equipment does not fit anatomy or occupied slot")
                    })?;
            }
        }
        game.activate_initial_floor().map_err(|error| {
            crate::Failure::simulation_failure(error)
                .unwrap_or_else(|| fail(format!("Initial floor activation: {error:?}")))
        })
    }
}

/// Recovery-only source binding. Scope reconstructions are cached across all
/// retained boundaries; current mutable terrain is deliberately never compared.
/// None of this work runs on the forward action path.
pub(crate) struct PopulationConfiguration<'a> {
    package: &'a Package,
    seed: u64,
    plan: Option<runtime::PopulationPlan>,
    actor_floor: u64,
    item_floor: u64,
    capabilities: runtime::ScenarioCapabilities,
    expected_scopes: std::cell::RefCell<BTreeMap<String, runtime::PreparedScope>>,
}

impl PopulationConfiguration<'_> {
    pub(crate) fn valid_game(&self, game: &Game) -> bool {
        if game.seed() != self.seed
            || !game.population_configuration_matches(
                self.plan.as_ref(),
                &self.capabilities,
                Some(tor_simulation::ActorId(self.package.selected)),
            )
        {
            return false;
        }
        let mut scopes = BTreeSet::new();
        for scope in game.activation_scopes() {
            if !self.valid_scope(scope) {
                return false;
            }
            scopes.insert(scope.scope.as_str());
        }
        let Some(plan) = &self.package.manifest.floor_activation else {
            return scopes.is_empty();
        };
        // Missing metadata for a published-but-unentered group must not silently
        // disable activation. Unbuilt declarations are not materialized evidence.
        for region in game
            .loaded_regions()
            .chain(game.detached_records().map(|(region, _)| region))
        {
            for (family_id, family) in &self.package.manifest.procedural_worlds {
                if plan.families.contains_key(family_id) {
                    if let Some((depth, _)) = family.address(region.0) {
                        if !scopes.contains(format!("{family_id}/{depth}").as_str()) {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    /// Bind historical receipts too: an abandoned branch may no longer retain
    /// the generated scope in the current Game, but its origin still belongs to
    /// the pinned family/profile, immutable placement geometry and population
    /// identity range. Authored reservations and the selected kit are excluded
    /// independently of whether their entities still exist.
    pub(crate) fn valid_record(&self, record: &runtime::FloorActivationRecord) -> bool {
        if !self.ensure_scope(&record.scope) {
            return false;
        }
        let scopes = self.expected_scopes.borrow();
        let scope = &scopes[&record.scope];
        let valid_attempt = |attempt: &runtime::PlacementDecision| match attempt {
            runtime::PlacementDecision::Skipped { .. } => true,
            runtime::PlacementDecision::Placed {
                location, entity, ..
            } => {
                entity
                    .actor_birth()
                    .is_none_or(|id| id.0 >= self.actor_floor)
                    && entity.item_birth().is_none_or(|id| id.0 >= self.item_floor)
                    && scope.cells.binary_search(location).is_ok()
                    && !scope.protected.contains(location)
            }
        };
        record.profile == scope.profile
            && record.decisions.iter().all(|decision| {
                if let runtime::StageResult::RoomsEvaluated { rooms } = &decision.result {
                    if rooms
                        .iter()
                        .map(|room| room.room)
                        .ne(scope.rooms.iter().map(|room| room.id))
                    {
                        return false;
                    }
                    for (decision, group) in rooms.iter().zip(&scope.rooms) {
                        if let runtime::RoomResult::Attempt {
                            decision: runtime::PlacementDecision::Placed { location, .. },
                        } = &decision.result
                        {
                            if group.cells.binary_search(location).is_err() {
                                return false;
                            }
                        }
                    }
                }
                decision.result.attempts().all(valid_attempt)
            })
    }

    fn valid_scope(&self, scope: &runtime::PreparedScope) -> bool {
        self.ensure_scope(&scope.scope)
            && self.expected_scopes.borrow().get(&scope.scope) == Some(scope)
    }

    fn ensure_scope(&self, identity: &str) -> bool {
        if self.expected_scopes.borrow().contains_key(identity) {
            return true;
        }
        let expected = (|| {
            let (family_id, depth) = identity.rsplit_once('/')?;
            let depth: u32 = depth.parse().ok()?;
            if depth == 0 || identity != format!("{family_id}/{depth}") {
                return None;
            }
            let family = self.package.manifest.procedural_worlds.get(family_id)?;
            // Validate this address against the family's finite ID domain before
            // reconstructing its pure geometry.
            family.group(family_id, self.seed, depth).ok()?;
            let definitions = self.package.generated_group(identity, self.seed).ok()?;
            let members: Vec<_> = definitions.regions.values().collect();
            self.package
                .activation_scope(identity, depth, &members, &definitions.rooms)
                .ok()?
        })();
        let Some(expected) = expected else {
            return false;
        };
        self.expected_scopes
            .borrow_mut()
            .insert(identity.into(), expected);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn fixture() -> Package {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
        let mut source = serde_json::to_value(resolve_source(&root).unwrap().manifest).unwrap();
        source["archetypes"] = json!({"marker":{"name":"marker"}});
        source["floor_activation"] = json!({"version":1,"protagonist":"selected_character",
            "families":{"dungeon":"plain"},"profiles":{"plain":{"version":1,"stages":[
            {"kind":"items","id":"stock","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{"archetype":"marker","weight":1}]}]}]}}});
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap()
    }

    fn changed(package: &Package, edit: impl FnOnce(&mut Value)) -> Result<Package, Failure> {
        let mut value = serde_json::to_value(&package.manifest).unwrap();
        edit(&mut value);
        Package::from_parts(
            serde_json::from_value(value).map_err(|error| fail(error.to_string()))?,
            vec![],
            &package.rules_source,
        )
    }

    fn carried_fixture() -> Package {
        changed(&fixture(), |value| {
            value["archetypes"]["stock"] = json!({"name":"finite stock","stackable":true});
            value["floor_activation"]["carried_item_pools"] = json!({
                "supplies":{"version":1,"choices":[
                    {"archetype":"marker","weight":2,"quantity":1},
                    {"archetype":"stock","weight":3,"quantity":7}
                ]}
            });
            value["floor_activation"]["profiles"]["plain"]["stages"] = json!([{
                "kind":"actors","id":"inhabitants","version":1,"count":[1,1],
                "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
                    "archetype":"marker","weight":1,
                    "carried_loot":{"version":1,"pool":"supplies",
                        "chance":{"numerator":1,"denominator":5}}
                }]}]
            }]);
        })
        .unwrap()
    }

    fn repeated_population_package() -> Package {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let mut package = read_package(&root).unwrap();
        let activation = package.manifest.floor_activation.as_mut().unwrap();
        let (name, mut profile) = activation
            .profiles
            .iter()
            .next()
            .map(|(id, p)| (id.clone(), p.clone()))
            .unwrap();
        profile
            .stages
            .retain(|stage| matches!(stage, PopulationStage::Actors { .. }));
        profile.stages.truncate(1);
        let PopulationStage::Actors { bands, .. } = &mut profile.stages[0] else {
            unreachable!()
        };
        let mut first = bands[0].clone();
        first.choices.truncate(1);
        *bands = (1..=16)
            .map(|depth| {
                let mut band = first.clone();
                band.depth = PopulationDepth::Finite {
                    minimum: depth,
                    maximum: depth,
                };
                band.choices[0].weight = depth;
                band
            })
            .collect();
        activation.profiles = BTreeMap::from([(name, profile)]);
        package
    }

    fn first_prepared_population_choice(
        plan: &runtime::PopulationPlan,
    ) -> &runtime::PreparedActorChoice {
        let runtime::PopulationContent::Actors { bands, .. } =
            &plan.profiles.values().next().unwrap().stages[0].population
        else {
            unreachable!()
        };
        &bands[0].choices[0]
    }

    #[test]
    fn population_birth_validation_counts_once_without_sharing_character_or_seed_witnesses() {
        let mut package = repeated_population_package();
        let characters: Vec<_> = package.manifest.characters.iter().map(|c| c.id).collect();
        let mut witnesses = 0;
        for selected in &characters {
            package.selected = *selected;
            for seed in [0, 1, 42] {
                let index = package.index(seed).unwrap();
                for _ in 0..2 {
                    let mut births = PopulationBirthValidation::new(&index);
                    let plan = package
                        .population_plan_with_births(&mut births)
                        .unwrap()
                        .unwrap();
                    assert_eq!(births.checks, 1);
                    assert_eq!(plan.protagonist, tor_simulation::ActorId(*selected));
                    let runtime::PopulationContent::Actors { bands, .. } =
                        &plan.profiles.values().next().unwrap().stages[0].population
                    else {
                        unreachable!()
                    };
                    assert_eq!(bands.len(), 16);
                    for (n, band) in bands.iter().enumerate() {
                        assert_eq!(band.choices[0].weight, n as u32 + 1);
                    }
                    witnesses += 1;
                }
            }
        }
        assert_eq!(witnesses, characters.len() * 6);
    }

    #[test]
    fn population_birth_validation_changed_recipe_and_failed_result_never_reuse_success() {
        let package = repeated_population_package();
        let index = package.index(42).unwrap();
        let plan = package.population_plan(&index).unwrap().unwrap();
        let original = first_prepared_population_choice(&plan);
        assert!(original.actor.build.is_some());
        for case in 0..3 {
            let mut births = PopulationBirthValidation::new(&index);
            births.validate(original).unwrap();
            let mut changed = original.clone();
            match case {
                0 => {
                    let mut build =
                        serde_json::to_value(changed.actor.build.as_ref().unwrap()).unwrap();
                    build["species"] = json!("missing-species");
                    changed.actor.build = Some(serde_json::from_value(build).unwrap());
                }
                1 => {
                    changed.depth_growth =
                        tor_simulation::prepared_spawn::DepthGrowth::RacialAfter { depth: 0 }
                }
                2 => changed
                    .advancement
                    .push(tor_simulation::prepared_spawn::OrdinalAdvancement {
                        ordinal: tor_simulation::HdCount(1),
                        choices: Default::default(),
                    }),
                _ => unreachable!(),
            }
            for expected in [2, 3] {
                assert!(births.validate(&changed).is_err(), "case {case}");
                assert_eq!(births.checks, expected);
            }
            births.validate(original).unwrap();
            assert_eq!(births.checks, 3);
            assert_eq!(births.successful.len(), 1);
        }
    }

    #[test]
    fn population_birth_validation_is_bounded_and_content_local() {
        let package = repeated_population_package();
        let index = package.index(42).unwrap();
        let plan = package.population_plan(&index).unwrap().unwrap();
        let original = first_prepared_population_choice(&plan);
        let mut births = PopulationBirthValidation::new(&index);
        for n in 0..=MAX_CHOICES {
            let mut choice = original.clone();
            choice.content = format!("recipe-{n}");
            births.validate(&choice).unwrap();
        }
        assert_eq!(births.successful.len(), MAX_CHOICES);
        assert_eq!(births.checks, MAX_CHOICES + 1);
        let other = fixture();
        let other_index = other.index(42).unwrap();
        let mut other_births = PopulationBirthValidation::new(&other_index);
        assert!(other_births.validate(original).is_err());
        assert_eq!(other_births.checks, 1);
        assert!(other_births.successful.is_empty());
    }

    #[test]
    fn activation_assets_require_effective_family_themes_even_without_a_spawn() {
        let package = changed(&carried_fixture(), |value| {
            value["themes"] = json!(["elsewhere"]);
            value["assets"] = json!({"elsewhere":["creature.marker","item.stock"], "floor":[]});
            value["zones"] = json!({"floor":{"themes":["floor"]}});
            value["procedural_worlds"]["dungeon"]["zone"] = json!("floor");
            value["archetypes"]["marker"]["asset"] = json!("creature.marker");
            value["archetypes"]["stock"]["asset"] = json!("item.stock");
            let stage = &mut value["floor_activation"]["profiles"]["plain"]["stages"][0];
            stage["count"] = json!([0, 0]);
            stage["bands"][0]["depth"]["minimum"] = json!(100);
            stage["bands"][0]["choices"][0]["carried_loot"]["chance"]["numerator"] = json!(0);
        })
        .unwrap();
        let error = package.check().unwrap_err();
        assert!(
            error
                .message
                .contains("Activation family dungeon: asset creature.marker"),
            "{error}"
        );
        let only_actor = changed(&package, |value| {
            value["assets"]["floor"] = json!(["creature.marker"]);
        })
        .unwrap();
        let error = only_actor.check().unwrap_err();
        assert!(
            error
                .message
                .contains("Activation family dungeon: asset item.stock"),
            "{error}"
        );
        let complete = changed(&package, |value| {
            value["assets"]["floor"] = json!(["creature.marker", "item.stock"]);
        })
        .unwrap();
        complete.check().unwrap();
    }

    #[test]
    fn activation_items_validate_the_concealed_pool_asset_only() {
        let package = changed(&fixture(), |value| {
            value["themes"] = json!(["floor"]);
            value["assets"] = json!({"floor":["item.secret"]});
            value["archetypes"]["marker"]["asset"] = json!("item.secret");
            value["archetypes"]["marker"]["appearance_pool"] = json!("vials");
            value["appearance_pools"] =
                json!({"vials":{"appearances":["cloudy vial"],"asset":"item.potion"}});
        })
        .unwrap();
        let error = package.check().unwrap_err();
        assert!(error.message.contains("asset item.potion"), "{error}");
        let complete = changed(&package, |value| {
            value["assets"]["floor"] = json!(["item.potion"]);
        })
        .unwrap();
        complete.check().unwrap();
        let palette = complete.palette(complete.manifest.themes.iter());
        assert!(palette.contains("item.potion"));
        assert!(!palette.contains("item.secret"));
    }

    #[test]
    fn carried_stock_compiles_every_pool_with_the_ordinary_item_adapter() {
        let package = carried_fixture();
        for (numerator, denominator) in [(0, 1), (1, 1), (1, 5), (5, 5)] {
            let package = changed(&package, |value| {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]
                    ["choices"][0]["carried_loot"]["chance"]["numerator"] = json!(numerator);
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]
                    ["choices"][0]["carried_loot"]["chance"]["denominator"] = json!(denominator);
            })
            .unwrap();
            package.check().unwrap();
            let index = package.index(42).unwrap();
            let plan = package.population_plan(&index).unwrap().unwrap();
            let encoded = serde_json::to_value(&plan).unwrap();
            let pool = &encoded["carried_item_pools"]["supplies"];
            assert_eq!(pool["version"], 1);
            for (position, archetype, quantity) in [(0, "marker", 1), (1, "stock", 7)] {
                let expected = index
                    .definitions
                    .population_item(archetype, quantity, index.seed, &index.appearances)
                    .unwrap();
                assert_eq!(
                    pool["choices"][position]["quantity"],
                    json!({"kind":"fixed","quantity":quantity})
                );
                assert_eq!(
                    pool["choices"][position]["spec"],
                    serde_json::to_value(expected).unwrap()
                );
            }
            let runtime::PopulationContent::Actors { bands, .. } =
                &plan.profiles["plain"].stages[0].population
            else {
                unreachable!()
            };
            let choice = serde_json::to_value(&bands[0].choices[0]).unwrap();
            assert_eq!(choice["carried_loot"]["chance"]["numerator"], numerator);
            assert_eq!(choice["carried_loot"]["chance"]["denominator"], denominator);
        }
        let package = changed(&package, |value| {
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                .as_object_mut()
                .unwrap()
                .remove("carried_loot");
        })
        .unwrap();
        let plan = package
            .population_plan(&package.index(42).unwrap())
            .unwrap()
            .unwrap();
        let runtime::PopulationContent::Actors { bands, .. } =
            &plan.profiles["plain"].stages[0].population
        else {
            unreachable!()
        };
        assert_eq!(
            serde_json::to_value(&bands[0].choices[0]).unwrap()["carried_loot"],
            Value::Null
        );
    }

    #[test]
    fn carried_stock_pins_concealed_ordinary_items_and_maximum_unused_pools() {
        let package = changed(&carried_fixture(), |value| {
            value["appearance_pools"] = json!({"hidden":{
                "appearances":["unmarked parcel"], "confounding":false, "asset":"item.hidden"
            }});
            value["archetypes"]["stock"]["appearance_pool"] = json!("hidden");
            value["archetypes"]["stock"]["asset"] = json!("item.secret");
            value["archetypes"]["stock"]["properties"] = json!({"quality":"fine"});
        })
        .unwrap();
        let index = package.index(42).unwrap();
        let plan = package.population_plan(&index).unwrap().unwrap();
        let spec = &plan.carried_item_pools["supplies"].choices[1].spec;
        assert!(spec.concealed && spec.stackable);
        assert_eq!(spec.appearance, "unmarked parcel");
        assert_eq!(spec.asset.as_deref(), Some("item.hidden"));
        assert_eq!(spec.properties["quality"], "fine");

        let package = changed(&fixture(), |value| {
            let choices: Vec<_> = (0..64)
                .map(|n| {
                    let name = format!("stock{n}");
                    value["archetypes"][&name] = json!({"name":name,"stackable":true});
                    json!({"archetype":name,"weight":1,"quantity":7})
                })
                .collect();
            value["floor_activation"]["carried_item_pools"] = Value::Object(
                (0..32)
                    .map(|n| (format!("pool{n}"), json!({"version":1,"choices":choices})))
                    .collect(),
            );
        })
        .unwrap();
        package.check().unwrap();
        let plan = package
            .population_plan(&package.index(42).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(plan.carried_item_pools.len(), 32);
        assert!(plan
            .carried_item_pools
            .values()
            .all(|pool| pool.choices.len() == 64));
    }

    #[test]
    fn carried_stock_rejects_invalid_even_unreachable_and_zero_chance_declarations() {
        let package = carried_fixture();
        for case in 0..19 {
            let invalid = changed(&package, |value| {
                // Neither this depth band nor a carry roll is reached at initial entry.
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["depth"]
                    ["minimum"] = json!(100);
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]
                    ["choices"][0]["carried_loot"]["chance"]["numerator"] = json!(0);
                let plan = &mut value["floor_activation"];
                match case {
                    0 => plan["carried_item_pools"]["supplies"]["version"] = json!(2),
                    1 => plan["carried_item_pools"]["supplies"]["choices"] = json!([]),
                    2 => {
                        plan["carried_item_pools"]["supplies"]["choices"][0]["quantity"] = json!(0)
                    }
                    3 => {
                        plan["carried_item_pools"]["supplies"]["choices"][0]["quantity"] = json!(2)
                    }
                    4 => plan["carried_item_pools"]["supplies"]["choices"][0]["weight"] = json!(0),
                    5 => {
                        plan["carried_item_pools"]["supplies"]["choices"][0]["weight"] =
                            json!(u32::MAX)
                    }
                    6 => {
                        plan["carried_item_pools"]["supplies"]["choices"][1]["archetype"] =
                            json!("marker")
                    }
                    7 => {
                        plan["carried_item_pools"]["supplies"]["choices"][0]["archetype"] =
                            json!("missing")
                    }
                    8 => {
                        plan["carried_item_pools"]["supplies"]["choices"] = json!(vec![
                            json!({"archetype":"stock","quantity":1,"weight":1});
                            65
                        ])
                    }
                    9 => {
                        plan["carried_item_pools"] = Value::Object(
                            (0..33)
                                .map(|n| {
                                    (
                                        format!("pool{n}"),
                                        json!({"version":1,"choices":[
                            {"archetype":"stock","quantity":1,"weight":1}]}),
                                    )
                                })
                                .collect(),
                        )
                    }
                    10 => {
                        plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                            ["carried_loot"]["pool"] = json!("missing")
                    }
                    11 => {
                        plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                            ["carried_loot"]["version"] = json!(2)
                    }
                    12 => {
                        plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                            ["carried_loot"]["chance"]["denominator"] = json!(0)
                    }
                    13 => {
                        plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"][0]
                            ["carried_loot"]["chance"]["numerator"] = json!(6)
                    }
                    14 => {
                        let choice = plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"]
                            [0]
                        .clone();
                        plan["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
                            json!([choice.clone(), choice]);
                    }
                    15 => {
                        let pool = plan["carried_item_pools"]["supplies"].clone();
                        plan["carried_item_pools"][""] = pool;
                    }
                    16 => value["archetypes"]["stock"]["properties"] = json!({"invalid":"\n"}),
                    17 => value["archetypes"]["stock"]["supply"] = json!("missing"),
                    18 => {
                        plan["carried_item_pools"]["unused"] = json!({"version":1,
                            "choices":[{"archetype":"marker","weight":1,"quantity":2}]});
                    }
                    _ => unreachable!(),
                }
            });
            assert!(
                invalid.is_err() || invalid.unwrap().build(42, false).is_err(),
                "invalid carried stock case {case}"
            );
        }
    }

    #[test]
    fn carried_stock_source_pool_map_rejects_duplicates_and_over_limit_before_compiling() {
        let pool = r#"{"version":1,"choices":[{"archetype":"marker","weight":1,"quantity":1}]}"#;
        let duplicate = format!(
            r#"{{"version":1,"protagonist":"selected_character","profiles":{{}},"carried_item_pools":{{"supplies":{pool},"supplies":{pool}}}}}"#
        );
        assert!(serde_json::from_str::<FloorActivation>(&duplicate).is_err());
        let too_many = json!({"version":1,"protagonist":"selected_character","profiles":{},
            "carried_item_pools":(0..33).map(|n| (format!("pool{n}"),
                json!({"version":1,"choices":[{"archetype":"marker","weight":1,"quantity":1}]})))
                .collect::<serde_json::Map<String, Value>>()});
        assert!(serde_json::from_value::<FloorActivation>(too_many).is_err());
        let duplicate_toml = r#"
version = 1
protagonist = "selected_character"
profiles = {}
[carried_item_pools.supplies]
version = 1
choices = [{ archetype = "marker", weight = 1, quantity = 1 }]
[carried_item_pools.supplies]
version = 1
choices = [{ archetype = "marker", weight = 1, quantity = 1 }]
"#;
        assert!(toml::from_str::<FloorActivation>(duplicate_toml).is_err());
    }

    #[test]
    fn carried_stock_source_fields_are_actor_choice_only_and_strict() {
        let actor = json!({"kind":"actors","id":"inhabitants","version":1,"count":[1,1],
        "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
            "archetype":"marker","weight":1,
            "carried_loot":{"version":1,"pool":"supplies","chance":{"numerator":1,"denominator":1}}
        }]}]});
        assert!(serde_json::from_value::<PopulationStage>(actor.clone()).is_ok());
        for case in 0..5 {
            let mut invalid = actor.clone();
            match case {
                0 => invalid["kind"] = json!("items"),
                1 => invalid["bands"][0]["choices"][0]["carried_loot"]["equipment_slot"] = json!(0),
                2 => {
                    invalid["bands"][0]["choices"][0]["carried_loot"]["chance"]["reroll"] =
                        json!(true)
                }
                3 => invalid["bands"][0]["choices"][0]["quantity"] = json!(1),
                4 => invalid["carried_loot"] = json!({"version":1}),
                _ => unreachable!(),
            }
            assert!(
                serde_json::from_value::<PopulationStage>(invalid).is_err(),
                "case {case}"
            );
        }
    }

    #[test]
    fn activation_authoring_rejects_unknown_mismatched_duplicate_and_unbounded_inputs() {
        let package = fixture();
        package.check().unwrap();
        for case in 0..12 {
            let package = changed(&package, |value| match case {
                0 => value["floor_activation"]["version"] = json!(2),
                1 => value["floor_activation"]["protagonist"] = json!(2),
                2 => value["floor_activation"]["protagonist"] = json!("controller"),
                3 => value["floor_activation"]["families"] = json!({"unknown":"plain"}),
                4 => value["floor_activation"]["families"] = json!({"dungeon":"unknown"}),
                5 => value["floor_activation"]["profiles"]["plain"]["version"] = json!(2),
                6 => {
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["count"] =
                        json!([2, 1])
                }
                7 => {
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["count"] =
                        json!([257, 257])
                }
                8 => {
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
                        json!([{"archetype":"marker","weight":1}, {"archetype":"marker","weight":1}])
                }
                9 => {
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["bands"][0]["choices"] =
                        json!([{"archetype":"unknown","weight":1}])
                }
                10 => {
                    let stage = value["floor_activation"]["profiles"]["plain"]["stages"][0].clone();
                    value["floor_activation"]["profiles"]["plain"]["stages"]
                        .as_array_mut()
                        .unwrap()
                        .push(stage);
                }
                11 => {
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["count"] =
                        json!([129, 129]);
                    let mut stage =
                        value["floor_activation"]["profiles"]["plain"]["stages"][0].clone();
                    stage["id"] = json!("second");
                    value["floor_activation"]["profiles"]["plain"]["stages"]
                        .as_array_mut()
                        .unwrap()
                        .push(stage);
                }
                _ => unreachable!(),
            })
            .unwrap();
            assert!(package.check().is_err(), "invalid population case {case}");
        }
    }

    #[test]
    fn population_stage_schema_is_typed_instead_of_an_expression_language() {
        let base = json!({"kind":"items","id":"stock","version":1,"count":[1,1],
            "bands":[{"depth":{"kind":"open","minimum":1},
            "choices":[{"archetype":"marker","weight":1}]}]});
        for case in 0..4 {
            let mut stage = base.clone();
            match case {
                0 => stage["kind"] = json!("script"),
                1 => stage["ai"] = json!("wander"),
                2 => {
                    stage["kind"] = json!("actors");
                    stage["quantity"] = json!(1);
                }
                3 => stage["when"] = json!("inventory.ration == 0"),
                _ => unreachable!(),
            }
            assert!(serde_json::from_value::<PopulationStage>(stage).is_err());
        }
    }

    #[test]
    fn room_gold_source_free_package_preserves_activated_and_unentered_scope_metadata() {
        let package = changed(&fixture(), |value| {
            value["archetypes"]["marker"]["stackable"] = json!(true);
            value["floor_activation"]["profiles"]["plain"]["stages"] = json!([{
                "kind":"room_items","id":"room-gold","version":1,
                "quantity":{"kind":"depth_scaled_uniform","minimum":2,"maximum_base":51,"per_depth":10},
                "chance":{"numerator":1,"denominator":2},
                "bands":[{"depth":{"kind":"open","minimum":1},"choices":[{"archetype":"marker","weight":1}]}]
            }]);
        }).unwrap();
        let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
        let restored = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
        assert!(restored.directory.is_none());
        assert!(restored.rules_source.is_empty());
        let mut warm = package.build(42, false).unwrap();
        let cold = restored.build(42, false).unwrap();
        assert_eq!(warm.activations_since(0), cold.activations_since(0));
        assert_eq!(
            warm.activation_scopes().collect::<Vec<_>>(),
            cold.activation_scopes().collect::<Vec<_>>()
        );
        let generated = package.generated_group("dungeon/2", 42).unwrap();
        let definitions: Vec<_> = generated.regions.values().collect();
        let unentered = package
            .activation_scope("dungeon/2", 2, &definitions, &generated.rooms)
            .unwrap()
            .unwrap();
        warm.register_activation_scope(unentered.clone()).unwrap();
        assert_eq!(warm.activation_count(), 1);
        let configuration = restored.population_configuration(42).unwrap();
        assert!(configuration.valid_game(&warm));
        for original in warm.activation_scopes() {
            assert!(configuration.valid_scope(original));
            let mut corrupt = original.clone();
            corrupt.rooms.remove(0);
            assert!(!configuration.valid_scope(&corrupt));
            let mut corrupt = original.clone();
            corrupt.rooms[0].id = 64;
            assert!(!configuration.valid_scope(&corrupt));
            let mut corrupt = original.clone();
            corrupt.rooms[0].cells.pop();
            assert!(!configuration.valid_scope(&corrupt));
        }
        let record = warm.activations_since(0).unwrap().remove(0);
        assert!(configuration.valid_record(&record));
        let mut missing_room = record;
        let runtime::StageResult::RoomsEvaluated { rooms } = &mut missing_room.decisions[0].result
        else {
            panic!()
        };
        rooms.remove(0);
        assert!(!configuration.valid_record(&missing_room));
        assert_eq!(
            crate::storage::schema::scenario::encode_content_package(&restored).unwrap(),
            bytes
        );
    }

    #[test]
    fn quantity_expressions_check_depth_overflow_and_stackability_before_population() {
        for quantity in [
            json!({"kind":"uniform","minimum":0,"maximum":10}),
            json!({"kind":"uniform","minimum":3,"maximum":2}),
            json!({"kind":"depth_scaled_uniform","minimum":2,"maximum_base":u64::MAX,"per_depth":1}),
            json!({"kind":"depth_scaled_uniform","minimum":2,"maximum_base":1,"per_depth":u64::MAX}),
        ] {
            let invalid = changed(&fixture(), |value| {
                value["floor_activation"]["profiles"]["plain"]["stages"][0]["quantity"] = quantity;
            })
            .unwrap();
            assert!(invalid.check().is_err());
        }
        let nonstackable = changed(&fixture(), |value| {
            value["floor_activation"]["profiles"]["plain"]["stages"][0]["quantity"] =
                json!({"kind":"uniform","minimum":1,"maximum":2});
        })
        .unwrap();
        assert!(nonstackable
            .population_plan(&nonstackable.index(42).unwrap())
            .is_err());
    }

    #[test]
    fn group_preparation_contains_only_canonical_immutable_activation_geometry() {
        let package = fixture();
        let index = package.index(42).unwrap();
        let first = package.build_group(42, &index, RegionId(1)).unwrap();
        let second = package.build_group(42, &index, RegionId(1)).unwrap();
        assert_eq!(first.records, second.records);
        assert_eq!(first.activation, second.activation);
        let scope = first.activation.unwrap();
        assert_eq!(scope.scope, "dungeon/1");
        assert_eq!(scope.profile, "plain");
        assert_eq!(scope.depth, 1);
        assert_eq!(scope.members.len(), 9);
        assert!(!scope.cells.is_empty());
        assert!(scope.cells.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(!scope.protected.is_empty());
        // A plan has an initial marker; scratch records have no such allocation.
        let mut plain = package.manifest.clone();
        plain.floor_activation = None;
        let plain = Package::from_parts(plain, vec![], &package.rules_source).unwrap();
        assert_eq!(
            first.records,
            plain
                .build_group(42, &plain.index(42).unwrap(), RegionId(1))
                .unwrap()
                .records
        );
    }

    #[test]
    fn omitted_character_namespace_is_reserved_before_dynamic_population() {
        let package = fixture();
        let mut package = changed(&package, |value| {
            value["characters"].as_array_mut().unwrap().push(json!({
                "id":900,"anchor":"dungeon/1/start","unselected":"omit"
            }));
        })
        .unwrap();
        let first = package.index(42).unwrap();
        assert!(first.ceilings.0 > 900);
        assert!(!first.homes.contains_key(&900));
        package.selected = 900;
        let alternate = package.index(42).unwrap();
        assert_eq!(first.ceilings.0, alternate.ceilings.0);
        assert!(!alternate.homes.contains_key(&1));
    }

    fn item_reservation_package(item_id: u64) -> Package {
        let package = changed(&fixture(), |value| {
            value["characters"][0]["loadout"] = json!([
                {"archetype":"marker","quantity":1},
                {"archetype":"marker","quantity":1}
            ]);
        })
        .unwrap();
        // Open families reserve their full address domain, independently of
        // the smaller materialization depth budget used by this fixture.
        let authored_region = package.manifest.procedural_worlds["dungeon"]
            .last_region()
            .unwrap()
            .checked_add(1)
            .unwrap();
        let region = serde_json::from_value(json!({
            "id":authored_region,"name":"Store room","size":[3,3,1],
            "items":[{"id":item_id,"at":[1,1,0],"archetype":"marker"}]
        }))
        .unwrap();
        Package::from_parts(package.manifest, vec![region], &package.rules_source).unwrap()
    }

    #[test]
    fn population_item_floor_includes_authored_reservations_and_selected_kit() {
        let package = item_reservation_package(100);
        let game = package.build(42, false).unwrap();
        let mut record = game.activations_since(0).unwrap().remove(0);
        let configuration = package.population_configuration(42).unwrap();
        assert!(configuration.valid_record(&record));
        let runtime::StageResult::Evaluated { attempts, .. } = &record.decisions[0].result else {
            panic!("fixture must evaluate its marker stage");
        };
        let runtime::PlacementDecision::Placed {
            entity: runtime::PopulationEntity::Item { id, .. },
            ..
        } = &attempts[0]
        else {
            panic!("fixture must place a population item");
        };
        assert_eq!(
            id.0, 103,
            "population starts immediately after both kit rows"
        );
        for reserved in [0, 100, 101, 102] {
            let runtime::StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result
            else {
                unreachable!();
            };
            let runtime::PlacementDecision::Placed { entity, .. } = &mut attempts[0] else {
                unreachable!();
            };
            *entity = runtime::PopulationEntity::Item {
                id: tor_simulation::ItemId(reserved),
                quantity: 1,
            };
            assert!(
                !configuration.valid_record(&record),
                "authored or selected-kit identity {reserved} is not a population origin"
            );
        }
    }

    #[test]
    fn carried_stock_nested_birth_rejects_authored_and_selected_kit_identity_aliases() {
        let package = item_reservation_package(100);
        let game = package.build(42, false).unwrap();
        let mut record = game.activations_since(0).unwrap().remove(0);
        let configuration = package.population_configuration(42).unwrap();
        for item in [103, 0, 100, 101, 102] {
            let runtime::StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result
            else {
                unreachable!()
            };
            let runtime::PlacementDecision::Placed { entity, .. } = &mut attempts[0] else {
                unreachable!()
            };
            *entity = runtime::PopulationEntity::Actor {
                id: tor_simulation::ActorId(configuration.actor_floor),
                carried_loot: Some(runtime::CarriedLootDecision::Carried {
                    content: "marker".into(),
                    quantity: 1,
                    item: tor_simulation::ItemId(item),
                }),
            };
            assert_eq!(
                configuration.valid_record(&record),
                item == 103,
                "nested item {item} must obey the same historical reservation floor"
            );
        }
        let runtime::StageResult::Evaluated { attempts, .. } = &mut record.decisions[0].result
        else {
            unreachable!()
        };
        let runtime::PlacementDecision::Placed { entity, .. } = &mut attempts[0] else {
            unreachable!()
        };
        *entity = runtime::PopulationEntity::Actor {
            id: tor_simulation::ActorId(configuration.actor_floor - 1),
            carried_loot: None,
        };
        assert!(
            !configuration.valid_record(&record),
            "ordinary actor floor remains enforced"
        );
    }

    #[test]
    fn population_item_floor_overflow_rejects_recovery_configuration() {
        let package = item_reservation_package(u64::MAX - 1);
        assert_eq!(package.index(42).unwrap().ceilings.1, u64::MAX);
        assert!(package.population_configuration(42).is_err());
    }

    #[test]
    fn initial_loadout_is_selected_only_and_uses_ordinary_quantity_validation() {
        let package = fixture();
        let package = changed(&package, |value| {
            value["characters"][0]["loadout"] = json!([{"archetype":"marker","quantity":2}]);
        })
        .unwrap();
        assert!(
            package.build(42, false).is_err(),
            "non-stackable loadout quantity rejected"
        );
        let package = changed(&package, |value| {
            value["characters"][0]["loadout"][0]["quantity"] = json!(1);
            value["characters"][0]["loadout"][0]["equipped_slot"] = json!(0);
        })
        .unwrap();
        assert!(
            package.build(42, false).is_err(),
            "non-equipment loadout cannot equip"
        );
    }
}

#[cfg(test)]
#[path = "population_depth_authoring_tests.rs"]
mod population_depth_authoring_tests;
