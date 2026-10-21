//! Immutable manifest definitions prepared for deterministic region construction.
//! Region instances remain lazy; this context contains no mutable game state and
//! is reconstructed rather than serialized into a save.
use super::{fail, label, require, Actor, Archetype, Failure, Item, Manifest};
use crate::creature_authoring::{BuildSpec, CompiledCatalog, Recipe};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug)]
pub(super) enum RecipeError {
    Invalid(Failure),
    UnknownFaction(String),
}
impl From<Failure> for RecipeError {
    fn from(failure: Failure) -> Self {
        Self::Invalid(failure)
    }
}
impl RecipeError {
    pub fn into_failure(
        self,
        origin: super::diagnostics::Origin<'_>,
        source: impl FnOnce() -> Option<Arc<str>>,
    ) -> Failure {
        match self {
            Self::Invalid(failure) => origin.context(failure),
            Self::UnknownFaction(faction) => {
                let source = source();
                origin.reference_path(
                    fail("Unknown creature faction"),
                    source.as_deref(),
                    &[
                        super::diagnostics::PathSegment::Field("creature"),
                        super::diagnostics::PathSegment::Field("faction"),
                    ],
                    super::diagnostics::ReferenceValue::Text(&faction),
                )
            }
        }
    }
}

fn prepare_recipe(
    author: Option<&BuildSpec>,
    anatomy: bool,
    catalog: &CompiledCatalog,
    manifest: &Manifest,
) -> Result<Option<Recipe>, RecipeError> {
    let Some(author) = author else {
        return Ok(None);
    };
    require(!anatomy, "Creature builds cannot also specify anatomy")?;
    let recipe = catalog.prepare(author)?;
    if !manifest.factions.is_empty() && !manifest.factions.contains_key(&author.faction) {
        return Err(RecipeError::UnknownFaction(author.faction.clone()));
    }
    Ok(Some(recipe))
}

#[derive(Clone, Debug)]
pub(super) enum AiReference {
    Resolved(Arc<super::AiProfile>),
    // Defer reporting until the original AI configuration boundary, so earlier
    // combat/geometry failures retain their precedence. Names are diagnostic only.
    Missing(String),
}

#[derive(Clone, Debug)]
pub(super) enum PreparedControl {
    External,
    Omitted,
    Ai(AiReference),
}

impl PreparedControl {
    fn ai(name: Option<&String>, profiles: &BTreeMap<String, Arc<super::AiProfile>>) -> Self {
        Self::Ai(match name.and_then(|name| profiles.get(name)) {
            Some(profile) => AiReference::Resolved(profile.clone()),
            None if name.is_none() => AiReference::Resolved(Arc::new(super::AiProfile::default())),
            None => AiReference::Missing(name.cloned().expect("named profile")),
        })
    }

    pub fn spawned(&self) -> bool {
        match self {
            Self::External | Self::Ai(_) => true,
            Self::Omitted => false,
        }
    }

    pub fn profile(&self) -> Result<Option<&super::AiProfile>, MissingAiReference<'_>> {
        match self {
            Self::Ai(AiReference::Resolved(profile)) => Ok(Some(profile)),
            Self::Ai(AiReference::Missing(name)) => Err(MissingAiReference { name }),
            Self::External | Self::Omitted => Ok(None),
        }
    }
}

#[derive(Debug)]
pub(super) struct MissingAiReference<'a> {
    pub name: &'a str,
}

#[derive(Debug)]
pub(super) struct MissingArchetype<'a> {
    pub name: &'a str,
}

impl From<MissingArchetype<'_>> for Failure {
    fn from(reference: MissingArchetype<'_>) -> Self {
        fail(format!("Unknown archetype {}", reference.name))
    }
}

/// Runtime definitions only. Inherited definitions are borrowed until installed;
/// inline authoring values are converted once and then moved into the game.
#[derive(Debug)]
pub(super) struct PreparedCreature<'a> {
    pub recipe: Option<Cow<'a, Recipe>>,
    pub anatomy: Option<Cow<'a, tor_simulation::AnatomySpec>>,
    pub known_identities: Vec<String>,
    pub body: Option<Cow<'a, tor_simulation::BodySpec>>,
    pub control: PreparedControl,
    pub velocity: Option<[i64; 3]>,
    pub asset: Option<Cow<'a, str>>,
}

impl PreparedCreature<'_> {
    pub fn borrowed(&self) -> PreparedCreature<'_> {
        PreparedCreature {
            recipe: self.recipe.as_deref().map(Cow::Borrowed),
            anatomy: self.anatomy.as_deref().map(Cow::Borrowed),
            known_identities: self.known_identities.clone(),

            body: self.body.as_deref().map(Cow::Borrowed),
            control: self.control.clone(),
            velocity: self.velocity,
            asset: self.asset.as_deref().map(Cow::Borrowed),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct PreparedArchetype {
    pub recipe: Option<Recipe>,
    pub anatomy: Option<tor_simulation::AnatomySpec>,
    pub equipment: Option<tor_simulation::EquipmentSpec>,
    pub projectile: Option<tor_simulation::projectile_profiles::ProjectileSpec>,
    pub equipment_generation: Option<tor_simulation::EquipmentGeneration>,
    pub consumable: Option<tor_simulation::ConsumableSpec>,
    pub readable: Option<tor_simulation::ReadableSpec>,
    pub device: Option<tor_simulation::DeviceSpec>,
    pub edible: Option<tor_simulation::EdibleSpec>,
    pub supply: Option<tor_simulation::content_ids::SupplyId>,
    pub magical: bool,
    pub currency: Option<tor_simulation::CurrencyKind>,
    pub class: tor_simulation::ItemClass,
    pub body: Option<tor_simulation::BodySpec>,
    pub identity: Option<String>,
    pub concealed: bool,
    pub stackable: bool,
    pub properties: BTreeMap<String, String>,
    pub name: Option<String>,
    pub turn_ticks: Option<u64>,
    pub actor_asset: Option<String>,
    pub item_asset: Option<String>,
}

impl PreparedArchetype {
    fn new(
        author: &Archetype,
        manifest: &Manifest,
        catalog: &CompiledCatalog,
    ) -> Result<Self, RecipeError> {
        Ok(Self {
            recipe: prepare_recipe(
                author.creature.as_ref(),
                author.anatomy.is_some(),
                catalog,
                manifest,
            )?,
            anatomy: author
                .anatomy
                .as_ref()
                .map(|anatomy| anatomy.compile(catalog.content().rules()))
                .transpose()?,
            equipment: author
                .equipment
                .as_ref()
                .map(|equipment| equipment.compile(catalog.content().rules()))
                .transpose()?,
            projectile: author
                .projectile
                .as_ref()
                .map(|value| value.compile(catalog.content().rules()))
                .transpose()?,
            equipment_generation: author
                .equipment_generation
                .map(super::EquipmentGeneration::compile)
                .transpose()?,
            consumable: author
                .consumable
                .as_ref()
                .map(|consumable| consumable.compile(catalog.content().rules()))
                .transpose()?,
            readable: author
                .readable
                .as_ref()
                .map(|readable| readable.compile(catalog.content().rules()))
                .transpose()?,
            device: author
                .device
                .as_ref()
                .map(|device| device.compile(catalog.content().rules()))
                .transpose()?,
            edible: author.edible.clone(),
            supply: author
                .supply
                .as_ref()
                .map(|value| {
                    let id = tor_simulation::content_ids::SupplyId::new(value)
                        .map_err(|_| fail(format!("Invalid supply ID {value:?}")))?;
                    require(
                        catalog.content().rules().supply(&id).is_some(),
                        "Unknown supply",
                    )?;
                    require(
                        author.equipment.is_none(),
                        "Equipment cannot also be supply content",
                    )?;
                    Ok::<_, Failure>(id)
                })
                .transpose()?,
            magical: author.magical,
            currency: author.currency.map(Into::into),
            class: author.class.into(),

            body: author.body.clone().map(Into::into),
            identity: author.identity.clone(),
            concealed: author.appearance_pool.is_some(),
            stackable: author.stackable,
            properties: author.properties.clone(),
            name: author.name.clone(),
            turn_ticks: author.turn_ticks,
            actor_asset: author.asset.clone(),
            item_asset: item_asset(author, manifest).map(str::to_owned),
        })
    }
}

/// Concealed items use their appearance pool's asset, including an intentional
/// absence. Never fall back to the identity-revealing archetype asset.
pub(super) fn item_asset<'a>(author: &'a Archetype, manifest: &'a Manifest) -> Option<&'a str> {
    match &author.appearance_pool {
        Some(pool) => manifest.appearance_pools.get(pool)?.asset.as_deref(),
        None => author.asset.as_deref(),
    }
}

#[derive(Debug)]
pub(super) struct PreparedCharacter {
    pub id: u64,
    pub anchor: String,
    pub turn_ticks: u64,
    pub creature: PreparedCreature<'static>,
    pub known_identities: Vec<String>,
}

#[derive(Debug)]
pub(super) struct PreparedDefinitions {
    catalog: CompiledCatalog,
    factions: BTreeSet<String>,
    archetypes: BTreeMap<String, PreparedArchetype>,
    ordinary: PreparedArchetype,
    pub characters: Vec<PreparedCharacter>,
    ai_profiles: BTreeMap<String, Arc<super::AiProfile>>,
    omitted: BTreeSet<u64>,
    pub objective_item: Option<u64>,
}

/// Prepare ordinary recoverable ammunition before any creature build captures
/// Content. Reuse the exact normal item adapter, without allocating an entity.
pub(super) fn compile_trap_ammunition(
    manifest: &Manifest,
    catalog: CompiledCatalog,
) -> Result<CompiledCatalog, Failure> {
    let archetypes = manifest
        .archetypes
        .iter()
        .filter(|(_, author)| {
            author.projectile.is_some()
                && author.stackable
                && !author.magical
                && author.appearance_pool.is_none()
                && author.equipment_generation.is_none()
        })
        .map(|(name, author)| {
            PreparedArchetype::new(author, manifest, &catalog)
                .map(|prepared| (name.clone(), prepared))
                .map_err(|error| {
                    error.into_failure(
                        super::diagnostics::Origin::Archetype {
                            file: "scenario.toml",
                            name,
                            catalog: "archetypes",
                        },
                        || None,
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let definitions = PreparedDefinitions {
        catalog,
        factions: BTreeSet::new(),
        archetypes,
        ordinary: PreparedArchetype::default(),
        characters: vec![],
        ai_profiles: BTreeMap::new(),
        omitted: BTreeSet::new(),
        objective_item: None,
    };
    let ammunition = definitions
        .archetypes
        .keys()
        .map(|key| {
            definitions
                .population_item(key, 1, 0, &BTreeMap::new())
                .map(|spec| (key.clone(), spec))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    definitions.catalog.with_trap_ammunition(ammunition)
}

impl PreparedDefinitions {
    pub fn creature_catalog(&self) -> &CompiledCatalog {
        &self.catalog
    }

    #[cfg(test)]
    pub fn new(manifest: &Manifest, selected: u64) -> Result<Self, Failure> {
        Self::new_with_source(
            manifest,
            selected,
            crate::scenario_package::parse_rules(
                include_str!("../../simulation/data/default-ruleset.toml"),
                "test rules",
            )
            .unwrap(),
            || None,
        )
    }
    #[cfg(test)]
    pub fn new_with_source(
        manifest: &Manifest,
        selected: u64,
        rules: Arc<tor_simulation::rules::Ruleset>,
        source: impl Fn() -> Option<Arc<str>>,
    ) -> Result<Self, Failure> {
        Self::new_with_sources(manifest, selected, rules, &BTreeMap::new(), |_| source())
    }

    pub fn new_with_sources(
        manifest: &Manifest,
        selected: u64,
        rules: Arc<tor_simulation::rules::Ruleset>,
        provenance: &BTreeMap<String, String>,
        source: impl Fn(&str) -> Option<Arc<str>>,
    ) -> Result<Self, Failure> {
        let catalog = manifest
            .creatures
            .compile_with_context(rules, |kind, id, failure| {
                super::source::catalog_context(provenance, kind, id, failure)
            })?;
        let catalog = compile_trap_ammunition(manifest, catalog)?;
        let ai_profiles: BTreeMap<_, _> = manifest
            .ai_profiles
            .iter()
            .map(|(name, author)| (name.clone(), Arc::new(author.clone())))
            .collect();
        let characters: Vec<_> =
            manifest
                .characters
                .iter()
                .map(|author| {
                    require(
                        manifest.objective.is_none() || author.creature.is_some(),
                        "Objectives require declared creature builds",
                    )
                    .map_err(|failure| {
                        super::diagnostics::Origin::Character(author.id).context(failure)
                    })?;
                    Ok(PreparedCharacter {
                        id: author.id,
                        anchor: author.anchor.clone(),
                        turn_ticks: author.turn_ticks,
                        creature: PreparedCreature {
                            recipe: prepare_recipe(
                                author.creature.as_ref(),
                                author.anatomy.is_some(),
                                &catalog,
                                manifest,
                            )
                            .map_err(|failure| {
                                failure.into_failure(
                                    super::diagnostics::Origin::Character(author.id),
                                    || source("scenario.toml"),
                                )
                            })?
                            .map(Cow::Owned),
                            anatomy: author
                                .anatomy
                                .as_ref()
                                .map(|anatomy| {
                                    anatomy.compile(catalog.content().rules()).map(Cow::Owned)
                                })
                                .transpose()?,
                            known_identities: author.known_identities.clone(),

                            body: author.body.clone().map(|body| Cow::Owned(body.into())),
                            control: if author.id == selected
                                && !manifest.arena.as_ref().is_some_and(|arena| {
                                    arena.control == super::ArenaControl::AllAi
                                }) {
                                PreparedControl::External
                            } else if author.unselected == "ai" {
                                PreparedControl::ai(author.ai.as_ref(), &ai_profiles)
                            } else {
                                PreparedControl::Omitted
                            },
                            velocity: author.velocity,
                            asset: author.asset.clone().map(Cow::Owned),
                        },
                        known_identities: author.known_identities.clone(),
                    })
                })
                .collect::<Result<_, Failure>>()?;
        let omitted = characters
            .iter()
            .filter(|c| !c.creature.control.spawned())
            .map(|c| c.id)
            .collect();
        Ok(Self {
            archetypes: manifest
                .archetypes
                .iter()
                .map(|(name, author)| {
                    PreparedArchetype::new(author, manifest, &catalog)
                        .map(|prepared| (name.clone(), prepared))
                        .map_err(|failure| {
                            let file =
                                super::source::declaration_file(provenance, "archetypes", name);
                            failure.into_failure(
                                super::source::archetype_origin(provenance, name),
                                || source(file),
                            )
                        })
                })
                .collect::<Result<_, Failure>>()?,
            catalog,
            factions: manifest.factions.keys().cloned().collect(),
            ordinary: PreparedArchetype::default(),
            characters,
            ai_profiles,
            omitted,
            objective_item: manifest.objective.as_ref().and_then(|o| o.item),
        })
    }

    /// Borrow only catalog labels; listing never copies or instantiates HD rows.
    pub fn creature_archetypes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.archetypes.iter().filter_map(|(id, definition)| {
            definition
                .recipe
                .as_ref()
                .map(|recipe| (id.as_str(), recipe.identity().name.as_str()))
        })
    }

    pub fn archetype<'a>(
        &self,
        key: &'a Option<String>,
    ) -> Result<&PreparedArchetype, MissingArchetype<'a>> {
        match key {
            Some(key) => self
                .archetypes
                .get(key)
                .ok_or(MissingArchetype { name: key }),
            None => Ok(&self.ordinary),
        }
    }

    pub fn omitted_carrier(&self, carrier: Option<u64>) -> bool {
        carrier.is_some_and(|id| self.omitted.contains(&id))
    }

    pub fn actor(&self, author: &Actor) -> Result<PreparedCreature<'_>, RecipeError> {
        let inherited = self.archetype(&author.archetype).map_err(Failure::from)?;
        let recipe = author
            .creature
            .as_ref()
            .map(|build| {
                let recipe = self.catalog.prepare(build)?;
                if !self.factions.is_empty() && !self.factions.contains(&build.faction) {
                    return Err(RecipeError::UnknownFaction(build.faction.clone()));
                }
                Ok(recipe)
            })
            .transpose()?
            .map(Cow::Owned)
            .or_else(|| inherited.recipe.as_ref().map(Cow::Borrowed));
        require(
            recipe.is_none() || (author.anatomy.is_none() && inherited.anatomy.is_none()),
            "Creature builds cannot also specify anatomy",
        )?;
        Ok(PreparedCreature {
            recipe,
            anatomy: author
                .anatomy
                .as_ref()
                .map(|anatomy| {
                    anatomy
                        .compile(self.catalog.content().rules())
                        .map(Cow::Owned)
                })
                .transpose()?
                .or_else(|| inherited.anatomy.as_ref().map(Cow::Borrowed)),
            known_identities: author.known_identities.clone(),

            body: author
                .body
                .clone()
                .map(|body| Cow::Owned(body.into()))
                .or_else(|| inherited.body.as_ref().map(Cow::Borrowed)),
            control: if author.controller == "ai" {
                PreparedControl::ai(author.ai.as_ref(), &self.ai_profiles)
            } else {
                PreparedControl::External
            },
            velocity: author.velocity,
            asset: inherited.actor_asset.as_deref().map(Cow::Borrowed),
        })
    }

    /// Resolve a catalog item through the ordinary item-definition adapter.
    /// The zero authoring ID is only a resolver sentinel, never an entity allocation.
    pub fn population_item(
        &self,
        archetype: &str,
        quantity: u64,
        seed: u64,
        appearances: &BTreeMap<String, String>,
    ) -> Result<tor_simulation::ItemSpec, Failure> {
        self.item(
            &Item {
                id: 0,
                at: [0; 3],
                archetype: Some(archetype.into()),
                quantity,
                equipped_slot: None,
                class: None,
                stackable: None,
                properties: BTreeMap::new(),
                name: None,
                carried_by: None,
                seed_names: vec![],
            },
            seed,
            appearances,
        )
    }

    pub fn population_actor(
        &self,
        archetype: &str,
        weight: u32,
        ai: Option<&str>,
    ) -> Result<tor_simulation::floor_activation::PreparedActorChoice, Failure> {
        let key = Some(archetype.to_owned());
        let definition = self.archetype(&key).map_err(Failure::from)?;
        let turn_ticks = definition.turn_ticks.unwrap_or(100);
        require(turn_ticks > 0, "Zero population actor duration")?;
        let ai = ai
            .map(|name| {
                self.ai_profiles
                    .get(name)
                    .map(|profile| {
                        profile.resolve(
                            definition
                                .recipe
                                .as_ref()
                                .map_or_else(Default::default, Recipe::behavior),
                        )
                    })
                    .ok_or_else(|| fail(format!("Unknown population AI profile {name:?}")))
            })
            .transpose()?;
        require(
            ai.is_none() || definition.recipe.is_some(),
            "Population AI requires a creature build",
        )?;
        Ok(tor_simulation::floor_activation::PreparedActorChoice {
            advancement: vec![],
            depth_growth: tor_simulation::prepared_spawn::DepthGrowth::Fixed {},
            content: archetype.into(),
            weight,
            turn_ticks,
            carried_loot: None,
            actor: tor_simulation::prepared_spawn::PreparedActor {
                build: definition.recipe.as_ref().map(Recipe::recorded),
                nutrition: definition.recipe.as_ref().map_or(
                    tor_simulation::nutrition::NutritionRequirement::Exempt,
                    Recipe::nutrition,
                ),
                identity: definition
                    .recipe
                    .as_ref()
                    .map(|recipe| recipe.identity().clone()),
                anatomy: definition.anatomy.clone(),
                body: definition.body.clone(),
                ai,
                known_identities: vec![],
                velocity: None,
                asset: definition.actor_asset.clone(),
            },
        })
    }

    pub fn item(
        &self,
        author: &Item,
        seed: u64,
        appearances: &BTreeMap<String, String>,
    ) -> Result<tor_simulation::ItemSpec, Failure> {
        let inherited = self.archetype(&author.archetype).map_err(Failure::from)?;
        require(
            !inherited.concealed
                || author
                    .class
                    .is_none_or(|class| tor_simulation::ItemClass::from(class) == inherited.class),
            "Concealed items cannot override their physical class",
        )?;
        let name = if author.seed_names.is_empty() {
            author
                .name
                .clone()
                .or_else(|| inherited.name.clone())
                .ok_or_else(|| fail("Item needs a name or archetype"))?
        } else {
            require(
                author.name.is_none(),
                "Item cannot have both name and seed_names",
            )?;
            author.seed_names[(seed % author.seed_names.len() as u64) as usize].clone()
        };
        require(
            label(&name) && author.seed_names.iter().all(|s| label(s)),
            "Invalid item name",
        )?;
        let key = author.archetype.clone().unwrap_or_else(|| name.clone());
        let identity = inherited.identity.clone().unwrap_or_else(|| key.clone());
        require(
            !inherited.concealed || (author.name.is_none() && author.seed_names.is_empty()),
            "Concealed items cannot override their identity name",
        )?;
        let appearance = appearances
            .get(&identity)
            .cloned()
            .unwrap_or_else(|| name.clone());
        let stackable = author.stackable.unwrap_or(inherited.stackable);
        require(
            self.objective_item != Some(author.id) || !stackable,
            "Objective item instances must be non-stackable",
        )?;
        let mut properties = inherited.properties.clone();
        properties.extend(author.properties.clone());
        let spec = tor_simulation::ItemSpec {
            equipment: inherited.equipment.clone(),
            projectile: inherited.projectile.clone(),
            equipment_generation: inherited.equipment_generation,
            consumable: inherited.consumable.clone(),
            readable: inherited.readable.clone(),
            device: inherited.device.clone(),
            edible: inherited.edible.clone(),
            supply: inherited.supply.clone(),
            magical: inherited.magical,
            currency: inherited.currency,
            class: author.class.map(Into::into).unwrap_or(inherited.class),
            archetype: key,
            identity,
            name,
            appearance,
            concealed: inherited.concealed,
            stackable,
            properties,
            asset: inherited.item_asset.clone(),
        };
        require(
            spec.device.is_none() || (!stackable && author.quantity == 1),
            "Device instances must be non-stackable single items",
        )?;
        require(
            author.quantity > 0 && (stackable || author.quantity == 1),
            "Invalid item quantity or non-stackable count",
        )?;
        require(
            spec.properties.len() <= 32
                && spec
                    .properties
                    .iter()
                    .all(|(k, v)| label(k) && v.len() <= 80 && !v.chars().any(char::is_control)),
            "Invalid item properties",
        )?;
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn projectile_archetype_reaches_stackable_item_without_equipment() {
        let mut manifest = creature_manifest();
        let projectile = serde_json::from_value(serde_json::json!({"family":"fixture.arrow",
            "recovery":"consumed_on_hit", "damage":{"components":[{"category":"keen",
            "descriptor":null,"amount":{"type":"fixed","value":2}}],
            "primary":{"category":"keen","descriptor":null,"sides":null}}}))
        .unwrap();
        manifest.archetypes.insert(
            "arrow".into(),
            super::Archetype {
                class: super::super::ItemClass::Weapon,
                name: Some("Fixture arrow".into()),
                projectile: Some(projectile),
                stackable: true,
                equipment_generation: Some(super::super::EquipmentGeneration::Authored {
                    hit_enchantment: 1,
                    damage_enchantment: 2,
                }),
                ..Default::default()
            },
        );
        let definitions = super::PreparedDefinitions::new(&manifest, 1).unwrap();
        let item = serde_json::from_value(serde_json::json!({"id":100,"at":[1,1,0],
            "archetype":"arrow","quantity":12}))
        .unwrap();
        let compiled = definitions.item(&item, 42, &Default::default()).unwrap();
        assert!(compiled.equipment.is_none());
        assert!(compiled.stackable);
        assert_eq!(
            compiled.projectile.as_ref().unwrap().family.as_str(),
            "fixture.arrow"
        );
        assert!(compiled.equipment_generation.unwrap().valid_for(
            compiled.class,
            compiled.equipment.as_ref(),
            compiled.projectile.as_ref()
        ));
    }
    #[test]
    fn authored_supply_identity_compiles_and_rejects_unknown_or_equipment_supplies() {
        let mut manifest = creature_manifest();
        manifest.archetypes.insert(
            "bandage".into(),
            super::Archetype {
                supply: Some("bandage".into()),
                name: Some("Bandage".into()),
                stackable: true,
                ..Default::default()
            },
        );
        let definitions = super::PreparedDefinitions::new(&manifest, 1).unwrap();
        let item = serde_json::from_value(
            serde_json::json!({"id":100, "at":[1,1,0], "archetype":"bandage"}),
        )
        .unwrap();
        let compiled = definitions.item(&item, 42, &Default::default()).unwrap();
        assert_eq!(compiled.supply.unwrap().as_str(), "bandage");
        manifest.archetypes.get_mut("bandage").unwrap().supply = Some("unknown".into());
        assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
        let author = manifest.archetypes.get_mut("bandage").unwrap();
        author.supply = Some("bandage".into());
        author.equipment = Some(
            serde_json::from_value(serde_json::json!({"slot":"shield", "modifiers":[]})).unwrap(),
        );
        assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
    }
    fn arena_package() -> super::super::Package {
        let mut value = serde_json::to_value(creature_manifest()).unwrap();
        value["arena"] = serde_json::json!({
            "participants": [1, 2], "control": "all_ai", "ticks": 7, "actions": 100
        });
        let mut manifest: super::Manifest = serde_json::from_value(value).unwrap();
        manifest
            .ai_profiles
            .insert("practice".into(), Default::default());
        manifest.characters[0].unselected = "ai".into();
        manifest.characters[0].ai = Some("practice".into());
        let build = manifest.characters[0].creature.clone().unwrap();
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = super::super::read_package(&root).unwrap();
        let mut regions = template.region_defs().unwrap();
        regions[0].actors.push(
            serde_json::from_value(serde_json::json!({
                "id": 2, "at": [2,1,0], "creature": build,
                "controller": "ai", "ai": "practice"
            }))
            .unwrap(),
        );
        super::super::Package::from_parts(manifest, regions, &template.rules_source).unwrap()
    }

    #[test]
    fn authored_arena_uses_selected_ai_and_configured_simulation_limits() {
        let package = arena_package();
        let mut game = package.build(42, true).unwrap();
        assert_eq!(game.arena_run().unwrap().limits.ticks, 7);
        assert_eq!(game.next_ai_action().unwrap().0, tor_simulation::ActorId(1));
        for _ in 0..10 {
            if game.run_outcome().terminal {
                break;
            }
            let (actor, _) = game.next_ai_action().unwrap();
            game.admit_ai_intention(actor).unwrap();
            game.execute_next_intention().unwrap().outcome.unwrap();
        }
        assert_eq!(game.tick(), 7);
        assert_eq!(
            game.arena_run().unwrap().stop().cloned(),
            Some(tor_simulation::arena::ArenaStop::TickLimit)
        );
    }

    #[test]
    fn authored_arena_runs_through_streaming_engine_at_zero_loading_radius() {
        let mut scenario = crate::Scenario::two_room(42);
        scenario.actors.clear();
        scenario.package = Some(std::sync::Arc::new(arena_package()));
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let mut engine = crate::Engine::memory(scenario).unwrap();
        for _ in 0..10 {
            let Some((actor, _)) = engine.next_ai_action() else {
                break;
            };
            engine.advance_ai(actor).unwrap();
        }
        assert!(engine.next_ai_action().is_none());
        let observation =
            serde_json::to_value(engine.observation(tor_protocol::ActorId(1)).unwrap()).unwrap();
        assert_eq!(observation["tick"], serde_json::json!("7"));
    }

    #[test]
    fn authored_arena_session_executes_without_a_player_controller() {
        let mut scenario = crate::Scenario::two_room(42);
        scenario.actors.clear();
        scenario.package = Some(std::sync::Arc::new(arena_package()));
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let mut service = crate::session::Service::new(crate::Engine::memory(scenario).unwrap());
        let mut committed = 0;
        for _ in 0..10 {
            match service.step() {
                crate::session::Step::Progress => committed += 1,
                crate::session::Step::Blocked => break,
                _ => panic!("unattended arena cannot wait for output"),
            }
        }
        assert!(committed >= 2, "both AI participants must execute");
        assert!(matches!(service.step(), crate::session::Step::Blocked));
    }

    #[test]
    fn arena_pins_keep_remote_participants_active_at_zero_radius() {
        let template = arena_package();
        let mut regions = template.region_defs().unwrap();
        let actor = regions[0].actors.pop().unwrap();
        assert_eq!(actor.id, 2);
        regions[1].actors.push(actor);
        let package = super::super::Package::from_parts(
            template.manifest.clone(),
            regions,
            &template.rules_source,
        )
        .unwrap();
        let mut scenario = crate::Scenario::two_room(42);
        scenario.actors.clear();
        scenario.package = Some(std::sync::Arc::new(package));
        scenario.streaming = Some(crate::Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let mut engine = crate::Engine::memory(scenario).unwrap();
        let mut acted = std::collections::BTreeSet::new();
        for _ in 0..10 {
            let Some((actor, _)) = engine.next_ai_action() else {
                break;
            };
            acted.insert(actor);
            engine.advance_ai(actor).unwrap();
        }
        assert_eq!(
            acted,
            std::collections::BTreeSet::from([tor_protocol::ActorId(1), tor_protocol::ActorId(2)])
        );
        assert_eq!(
            serde_json::to_value(engine.observation(tor_protocol::ActorId(2)).unwrap()).unwrap()
                ["tick"],
            serde_json::json!("7")
        );
    }

    #[test]
    fn manual_arena_waits_for_selected_input_then_runs_other_ai_normally() {
        let template = arena_package();
        let mut manifest = template.manifest.clone();
        manifest.arena.as_mut().unwrap().control = super::super::ArenaControl::Manual;
        let package = super::super::Package::from_parts(
            manifest,
            template.region_defs().unwrap(),
            &template.rules_source,
        )
        .unwrap();
        let mut game = package.build(42, true).unwrap();
        assert!(!game.is_ai(tor_simulation::ActorId(1)));
        assert!(game.next_ai_action().is_none());
        assert_eq!(game.tick(), 0);
        game.act(tor_simulation::ActorId(1), tor_simulation::Action::Wait)
            .unwrap();
        let (actor, _) = game.next_ai_action().unwrap();
        assert_eq!(actor, tor_simulation::ActorId(2));
        game.admit_ai_intention(actor).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.tick(), 7);
        assert!(game.run_outcome().terminal);
    }

    #[test]
    fn authored_arena_rejects_invalid_participants_limits_and_controllers() {
        let template = arena_package();
        for case in 0..7 {
            let mut manifest = template.manifest.clone();
            let arena = manifest.arena.as_mut().unwrap();
            match case {
                0 => arena.participants.push(1),
                1 => arena.participants = vec![2],
                2 => arena.participants.push(99),
                3 => arena.ticks = 0,
                4 => arena.actions = 10001,
                5 => {
                    manifest.characters[0].unselected = "omit".into();
                    manifest.characters[0].ai = None;
                }
                6 => manifest.characters[0].creature = None,
                _ => unreachable!(),
            }
            let result = super::super::Package::from_parts(
                manifest,
                template.region_defs().unwrap(),
                &template.rules_source,
            )
            .and_then(|package| package.build(42, true));
            assert!(result.is_err(), "invalid arena case {case}");
        }
    }

    fn creature_manifest() -> super::Manifest {
        let mut manifest: super::Manifest =
            toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
        manifest.creatures = serde_json::from_value(serde_json::json!({"species": {"human": {
            "kind": "humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000}, "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"strikes":[{"offset":0,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]},
            "modifiers": [{"scope":"always", "op":{"kind":"grant_ability", "ability":"spark_lance_fire"}}]
        }}})).unwrap();
        manifest.characters[0].creature = Some(serde_json::from_value(serde_json::json!({
            "species": "human", "name": "test creature", "faction": "neutral", "binding": "intellect",
            "starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice": [{"source": "racial"}, {"source": "warrior"}]
        })).unwrap());
        manifest
    }

    #[test]
    fn creature_package_spawns_selected_and_inherited_actors_with_independent_health() {
        let mut manifest = creature_manifest();
        let build = manifest.characters[0].creature.clone().unwrap();
        manifest.archetypes.insert(
            "creature".into(),
            super::Archetype {
                creature: Some(build),
                ..Default::default()
            },
        );
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = super::super::read_package(&root).unwrap();
        let mut regions = template.region_defs().unwrap();
        regions[0].actors.push(
            serde_json::from_value(
                serde_json::json!({"id": 2, "at": [2, 1, 0], "archetype": "creature"}),
            )
            .unwrap(),
        );
        let package =
            super::super::Package::from_parts(manifest, regions, &template.rules_source).unwrap();
        let first = package.build(42, true).unwrap();
        let second = package.build(42, true).unwrap();
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let snapshot = serde_json::to_value(first.checkpoint(&mut shared)).unwrap();
        assert_eq!(
            snapshot["combat"]["selected"],
            serde_json::json!(1),
            "creature-backed character participates in run lifecycle"
        );
        let selected = first.creature(tor_simulation::ActorId(1)).unwrap();
        let mob = first.creature(tor_simulation::ActorId(2)).unwrap();
        assert_eq!(
            selected,
            second.creature(tor_simulation::ActorId(1)).unwrap()
        );
        assert_eq!(selected.build().choices(), mob.build().choices());
        assert_ne!(selected.build().ledger(), mob.build().ledger());
        assert!(selected
            .derived()
            .abilities
            .contains(&tor_simulation::content_ids::AbilityId::new("spark_lance_fire").unwrap()));
    }

    #[test]
    fn required_nutrition_survives_player_npc_and_population_recipe_construction() {
        use tor_simulation::nutrition::{Nutrition, NutritionPolicy, NutritionRequirement};
        let mut manifest = creature_manifest();
        manifest.characters[0].creature.as_mut().unwrap().nutrition =
            NutritionRequirement::Required;
        manifest.archetypes.insert(
            "creature".into(),
            super::Archetype {
                creature: manifest.characters[0].creature.clone(),
                ..Default::default()
            },
        );
        let mut document: tor_simulation::rules::RulesetDocument =
            toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
        document.nutrition = NutritionPolicy::Enabled {
            initial: 8,
            maximum: 12,
            hungry_below: 7,
            weak_below: 3,
            starve_at: -3,
            unit_ticks: 100,
            hungry_recovery: tor_simulation::health_recovery::RecoveryNutrition::Slowed {
                multiplier: 2,
            },
            weak_recovery: tor_simulation::health_recovery::RecoveryNutrition::Stopped,
            empty_recovery: tor_simulation::health_recovery::RecoveryNutrition::Stopped,
            regeneration_cost: tor_simulation::nutrition::RegenerationCost {
                denominator: 1,
                health: 0,
                stamina: 0,
                focus: 0,
                mana: 0,
            },
        };
        let rules_source = toml::to_string(&document).unwrap();
        let definitions = PreparedDefinitions::new_with_source(
            &manifest,
            1,
            tor_simulation::rules::Ruleset::validate(document).unwrap(),
            || None,
        )
        .unwrap();
        let prepared = definitions.population_actor("creature", 1, None).unwrap();
        assert_eq!(prepared.actor.nutrition, NutritionRequirement::Required);
        let restored: tor_simulation::floor_activation::PreparedActorChoice =
            serde_json::from_value(serde_json::to_value(&prepared).unwrap()).unwrap();
        assert_eq!(restored, prepared);
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let template = super::super::read_package(&root).unwrap();
        let mut regions = template.region_defs().unwrap();
        regions[0].actors.push(
            serde_json::from_value(serde_json::json!({
                "id":2, "at":[2,1,0], "archetype":"creature"
            }))
            .unwrap(),
        );
        let package = super::super::Package::from_parts(manifest, regions, &rules_source).unwrap();
        let mut game = package.build(42, true).unwrap();
        for actor in [tor_simulation::ActorId(1), tor_simulation::ActorId(2)] {
            assert_eq!(
                game.creature(actor).unwrap().nutrition(),
                Nutrition::Required {
                    reserve: 8,
                    elapsed: 0,
                    digestion_remainder: 0,
                    regeneration_remainder: 0
                }
            );
        }
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 3, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_prepared_actor(actor, &restored.actor, 42)
            .unwrap();
        assert_eq!(
            game.creature(actor).unwrap().nutrition(),
            Nutrition::Required {
                reserve: 8,
                elapsed: 0,
                digestion_remainder: 0,
                regeneration_remainder: 0
            }
        );
    }

    #[test]
    fn creature_validation_precedes_lazy_faction_diagnostics() {
        let mut manifest = creature_manifest();
        PreparedDefinitions::new_with_source(
            &manifest,
            1,
            crate::scenario_package::parse_rules(
                include_str!("../../simulation/data/default-ruleset.toml"),
                "test rules",
            )
            .unwrap(),
            || panic!("successful compilation must not acquire diagnostic source"),
        )
        .unwrap();
        manifest.factions.insert("other".into(), Default::default());
        let original = manifest.characters[0].creature.as_ref().unwrap().clone();
        manifest.characters[0]
            .creature
            .as_mut()
            .unwrap()
            .hit_dice
            .clear();
        let error = PreparedDefinitions::new_with_source(
            &manifest,
            1,
            crate::scenario_package::parse_rules(
                include_str!("../../simulation/data/default-ruleset.toml"),
                "test rules",
            )
            .unwrap(),
            || panic!("invalid choices precede faction source acquisition"),
        )
        .unwrap_err();
        assert!(error.message.contains("require 1 to 256 hit dice"));
        manifest.characters[0].creature = Some(original);
        let source: Arc<str> = toml::to_string(&manifest).unwrap().into();
        let line = source
            .lines()
            .position(|line| line.contains("faction = \"neutral\""))
            .expect("the creature faction is declared")
            + 1;
        let reads = std::cell::Cell::new(0);
        let error = PreparedDefinitions::new_with_source(
            &manifest,
            1,
            crate::scenario_package::parse_rules(
                include_str!("../../simulation/data/default-ruleset.toml"),
                "test rules",
            )
            .unwrap(),
            || {
                reads.set(reads.get() + 1);
                Some(source.clone())
            },
        )
        .unwrap_err();
        assert_eq!(reads.get(), 1);
        assert!(
            error.message.contains(&format!("scenario.toml:{line}:")),
            "{}",
            error.message
        );
        assert!(error.message.contains("Unknown creature faction"));
    }

    #[test]
    fn creature_preparation_rejects_external_anatomy_and_unknown_factions() {
        let mut manifest = creature_manifest();
        manifest.characters[0].anatomy = Some(super::super::AnatomySpec {
            slots: vec![],
            capacities: Default::default(),
            primary_attack_socket: super::super::PrimaryAttackSocket::None {},
        });
        assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
        manifest.characters[0].anatomy = None;
        manifest.factions.insert("other".into(), Default::default());
        assert!(super::PreparedDefinitions::new(&manifest, 1).is_err());
        manifest.factions.clear();
        manifest.objective = Some(super::super::Objective {
            anchor: "1/start".into(),
            item: None,
            disclosed: false,
            continue_play: true,
        });
        let prepared = super::PreparedDefinitions::new(&manifest, 1).unwrap();
        assert!(prepared.characters[0].creature.recipe.is_some());

        let actor: super::Actor = serde_json::from_value(serde_json::json!({"id": 2, "at": [2, 1, 0], "creature": manifest.characters[0].creature, "anatomy": {"slots": [], "capacities": {}, "primary_attack_socket": {"kind": "none"}}})).unwrap();
        assert!(prepared.actor(&actor).is_err());
    }
    use super::*;

    #[test]
    fn concealed_item_cannot_override_its_physical_class() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let package = super::super::read_package(&root).unwrap();
        let prepared = PreparedDefinitions::new(&package.manifest, 1).unwrap();
        let mut item: Item =
            toml::from_str("id = 99\nat = [1, 1, 0]\narchetype = 'healing'\nclass = 'weapon'\n")
                .unwrap();
        assert!(prepared.item(&item, 42, &BTreeMap::new()).is_err());
        item.class = None;
        let spec = prepared.item(&item, 42, &BTreeMap::new()).unwrap();
        assert_eq!(
            spec.class,
            package.manifest.archetypes["healing"].class.into()
        );
    }

    #[test]
    fn prepared_item_assets_keep_pool_absence_and_do_not_reveal_identity_assets() {
        let mut manifest: Manifest =
            toml::from_str(include_str!("../../../scenarios/tests/items/scenario.toml")).unwrap();
        manifest.archetypes.get_mut("arrow").unwrap().asset = Some("item.arrow".into());
        manifest.archetypes.get_mut("healing").unwrap().asset = Some("identity.healing".into());
        manifest.appearance_pools.get_mut("potions").unwrap().asset = Some("item.shared".into());
        let arrow = &manifest.archetypes["arrow"];
        assert_eq!(item_asset(arrow, &manifest), Some("item.arrow"));
        assert_eq!(
            PreparedArchetype::new(
                arrow,
                &manifest,
                &manifest
                    .creatures
                    .compile(
                        crate::scenario_package::parse_rules(
                            include_str!("../../simulation/data/default-ruleset.toml"),
                            "test rules"
                        )
                        .unwrap()
                    )
                    .unwrap()
            )
            .unwrap()
            .item_asset
            .as_deref(),
            Some("item.arrow")
        );
        let hidden = &manifest.archetypes["healing"];
        assert_eq!(item_asset(hidden, &manifest), Some("item.shared"));
        assert_eq!(
            PreparedArchetype::new(
                hidden,
                &manifest,
                &manifest
                    .creatures
                    .compile(
                        crate::scenario_package::parse_rules(
                            include_str!("../../simulation/data/default-ruleset.toml"),
                            "test rules"
                        )
                        .unwrap()
                    )
                    .unwrap()
            )
            .unwrap()
            .item_asset
            .as_deref(),
            Some("item.shared")
        );
        manifest.appearance_pools.get_mut("potions").unwrap().asset = None;
        let hidden = &manifest.archetypes["healing"];
        assert_eq!(item_asset(hidden, &manifest), None);
        assert_eq!(
            PreparedArchetype::new(
                hidden,
                &manifest,
                &manifest
                    .creatures
                    .compile(
                        crate::scenario_package::parse_rules(
                            include_str!("../../simulation/data/default-ruleset.toml"),
                            "test rules"
                        )
                        .unwrap()
                    )
                    .unwrap()
            )
            .unwrap()
            .item_asset,
            None
        );
    }
    use crate::scenario_package::{AiProfile, AppearancePool, BodySpec};
    use std::path::Path;

    #[test]
    fn prepared_controls_resolve_profiles_and_preserve_selection_and_omission() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut manifest = super::super::read_package(&root).unwrap().manifest;
        manifest.ai_profiles.insert(
            "careful".into(),
            AiProfile {
                care: Some(true),
                equipment: Some(true),
                loot: Some(true),
                flee_percent: Some(19),
                ..Default::default()
            },
        );
        let mut other = manifest.characters[0].clone();
        other.id = 2;
        other.unselected = "ai".into();
        other.ai = Some("careful".into());
        manifest.characters.push(other);
        let mut omitted = manifest.characters[0].clone();
        omitted.id = 3;
        manifest.characters.push(omitted);
        let prepared = PreparedDefinitions::new(&manifest, 1).unwrap();
        let first = &prepared.characters[0].creature.control;
        assert!(first.spawned() && first.profile().unwrap().is_none());
        assert!(prepared.characters[1].creature.control.spawned());
        assert!(!prepared.characters[2].creature.control.spawned());
        assert!(!prepared.omitted_carrier(Some(1)));
        assert!(!prepared.omitted_carrier(Some(2)));
        assert!(prepared.omitted_carrier(Some(3)));
        manifest
            .ai_profiles
            .get_mut("careful")
            .unwrap()
            .flee_percent = Some(99);
        assert_eq!(
            prepared.characters[1]
                .creature
                .control
                .profile()
                .unwrap()
                .unwrap()
                .flee_percent,
            Some(19)
        );
        let selected_other = PreparedDefinitions::new(&manifest, 2).unwrap();
        assert!(selected_other.characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .is_none());
        assert!(selected_other.omitted_carrier(Some(1)));
        manifest.characters[1].ai = None;
        let defaulted = PreparedDefinitions::new(&manifest, 1).unwrap();
        assert_eq!(
            defaulted.characters[1]
                .creature
                .control
                .profile()
                .unwrap()
                .unwrap(),
            &AiProfile::default()
        );
        assert!(
            PreparedDefinitions::new(&manifest, 2).unwrap().characters[1]
                .creature
                .control
                .profile()
                .unwrap()
                .is_none()
        );
        manifest.characters[1].ai = Some("missing".into());
        let deferred = PreparedDefinitions::new(&manifest, 1).unwrap();
        assert!(deferred.characters[1].creature.control.spawned());
        assert_eq!(
            deferred.characters[1]
                .creature
                .control
                .profile()
                .unwrap_err()
                .name,
            "missing"
        );
        // Selected characters are externally controlled even if their unused
        // unselected-mode reference is missing; preserve the existing policy.
        assert!(
            PreparedDefinitions::new(&manifest, 2).unwrap().characters[1]
                .creature
                .control
                .profile()
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn compilation_preserves_manifest_attributes_and_concealed_assets() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut manifest = super::super::read_package(&root).unwrap().manifest;
        manifest.ai_profiles.insert(
            "careful".into(),
            AiProfile {
                care: Some(true),
                equipment: Some(true),
                loot: Some(true),
                flee_percent: Some(19),
                ..Default::default()
            },
        );
        manifest.appearance_pools.insert(
            "hidden".into(),
            AppearancePool {
                appearances: vec!["unmarked".into()],
                confounding: false,
                asset: Some("item.hidden".into()),
            },
        );
        let definitions: Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        manifest.creatures = definitions.creatures;
        manifest
            .creatures
            .species
            .get_mut("figure")
            .unwrap()
            .modifiers = vec![crate::creature_authoring::Modifier {
            scope: crate::creature_authoring::Scope::Always,
            op: crate::creature_authoring::Op::Add {
                stat: crate::creature_authoring::Stat::Health,
                value: 33,
            },
        }];
        let mut creature = definitions.characters[0].creature.clone().unwrap();
        creature.name = "guard".into();
        manifest.archetypes.insert(
            "guard".into(),
            Archetype {
                creature: Some(creature),
                anatomy: None,
                equipment: None,
                projectile: None,
                equipment_generation: None,
                consumable: None,
                readable: None,
                device: None,
                edible: None,
                supply: None,
                magical: false,
                currency: None,
                class: Default::default(),
                body: Some(BodySpec {
                    cells: vec![[0, 0, 0]],
                    eye: [0, 0, 0],
                    mass: 91,
                }),
                identity: Some("guard-item".into()),
                appearance_pool: Some("hidden".into()),
                stackable: true,
                properties: BTreeMap::from([("quality".into(), "fine".into())]),
                name: Some("guard token".into()),
                turn_ticks: Some(73),
                asset: Some("creature.guard".into()),
            },
        );
        let prepared = PreparedDefinitions::new(&manifest, 1).unwrap();
        let guard = prepared.archetype(&Some("guard".into())).unwrap();
        let recipe = guard.recipe.as_ref().unwrap();
        assert_eq!(recipe.identity().name, "guard");
        assert_eq!(
            recipe
                .instantiate(42)
                .unwrap()
                .derive()
                .unwrap()
                .maximum_health,
            41
        );
        assert_eq!(guard.body.as_ref().unwrap().mass, 91);
        assert_eq!(guard.identity.as_deref(), Some("guard-item"));
        assert!(guard.concealed && guard.stackable);
        assert_eq!(
            guard.properties.get("quality").map(String::as_str),
            Some("fine")
        );
        assert_eq!(guard.name.as_deref(), Some("guard token"));
        assert_eq!(guard.turn_ticks, Some(73));
        assert_eq!(guard.actor_asset.as_deref(), Some("creature.guard"));
        assert_eq!(guard.item_asset.as_deref(), Some("item.hidden"));
        assert_eq!(prepared.ai_profiles["careful"].care, Some(true));
        assert_eq!(prepared.ai_profiles["careful"].flee_percent, Some(19));
        let ordinary = prepared.archetype(&None).unwrap();
        assert!(ordinary.recipe.is_none() && ordinary.body.is_none() && !ordinary.stackable);
        assert!(prepared.archetype(&Some("missing".into())).is_err());
    }

    #[test]
    fn objectives_do_not_invent_creature_builds() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut manifest = super::super::read_package(&root).unwrap().manifest;
        assert!(
            PreparedDefinitions::new(&manifest, 1).unwrap().characters[0]
                .creature
                .recipe
                .is_none()
        );
        manifest.objective = Some(super::super::Objective {
            anchor: "1/start".into(),
            item: Some(7),
            disclosed: true,
            continue_play: false,
        });
        let error = PreparedDefinitions::new(&manifest, 1).unwrap_err();
        assert!(error.message.contains("character 1"));
        assert!(error
            .message
            .contains("Objectives require declared creature builds"));
        let owned = creature_manifest();
        manifest.creatures = owned.creatures;
        manifest.characters[0].creature = owned.characters[0].creature.clone();
        let prepared = PreparedDefinitions::new(&manifest, 1).unwrap();
        assert_eq!(prepared.objective_item, Some(7));
        assert_eq!(
            prepared.characters[0]
                .creature
                .recipe
                .as_ref()
                .unwrap()
                .identity()
                .name,
            "test creature"
        );
    }
}
