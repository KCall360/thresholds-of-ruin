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
    Resolved(Arc<tor_simulation::ai::AiProfile>),
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
    fn ai(
        name: Option<&String>,
        profiles: &BTreeMap<String, Arc<tor_simulation::ai::AiProfile>>,
    ) -> Self {
        Self::Ai(match name.and_then(|name| profiles.get(name)) {
            Some(profile) => AiReference::Resolved(profile.clone()),
            None => AiReference::Missing(name.cloned().unwrap_or_else(|| "<unspecified>".into())),
        })
    }

    pub fn spawned(&self) -> bool {
        match self {
            Self::External | Self::Ai(_) => true,
            Self::Omitted => false,
        }
    }

    pub fn profile(
        &self,
    ) -> Result<Option<&tor_simulation::ai::AiProfile>, MissingAiReference<'_>> {
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
    pub consumable: Option<tor_simulation::ConsumableSpec>,
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
            anatomy: author.anatomy.clone().map(Into::into),
            equipment: author.equipment.clone().map(Into::into),
            consumable: author.consumable.clone().map(Into::into),
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
    ai_profiles: BTreeMap<String, Arc<tor_simulation::ai::AiProfile>>,
    omitted: BTreeSet<u64>,
    pub objective_item: Option<u64>,
}

impl PreparedDefinitions {
    pub fn creature_catalog(&self) -> &CompiledCatalog {
        &self.catalog
    }

    #[cfg(test)]
    pub fn new(manifest: &Manifest, selected: u64) -> Result<Self, Failure> {
        Self::new_with_source(manifest, selected, || None)
    }
    pub fn new_with_source(
        manifest: &Manifest,
        selected: u64,
        source: impl Fn() -> Option<Arc<str>>,
    ) -> Result<Self, Failure> {
        let catalog = manifest.creatures.compile()?;
        let ai_profiles: BTreeMap<_, _> = manifest
            .ai_profiles
            .iter()
            .map(|(name, author)| (name.clone(), Arc::new(author.clone().into())))
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
                                    &source,
                                )
                            })?
                            .map(Cow::Owned),
                            anatomy: author
                                .anatomy
                                .clone()
                                .map(|anatomy| Cow::Owned(anatomy.into())),
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
                            failure
                                .into_failure(super::diagnostics::Origin::Archetype(name), &source)
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
                .clone()
                .map(|anatomy| Cow::Owned(anatomy.into()))
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
            consumable: inherited.consumable.clone(),
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
        super::super::Package::from_parts(manifest, regions).unwrap()
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
            game.arena_run().unwrap().stop,
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
        let package =
            super::super::Package::from_parts(template.manifest.clone(), regions).unwrap();
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
        let package =
            super::super::Package::from_parts(manifest, template.region_defs().unwrap()).unwrap();
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
            let result =
                super::super::Package::from_parts(manifest, template.region_defs().unwrap())
                    .and_then(|package| package.build(42, true));
            assert!(result.is_err(), "invalid arena case {case}");
        }
    }

    fn creature_manifest() -> super::Manifest {
        let mut manifest: super::Manifest =
            toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
        manifest.creatures = serde_json::from_value(serde_json::json!({"species": {"human": {
            "kind": "humanoid", "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","descriptor":null,"sides":6},"components":[{"category":"impact","descriptor":null,"amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}},
            "grants": [{"type": "ability", "ability": "magic_bolt"}]
        }}})).unwrap();
        manifest.characters[0].creature = Some(serde_json::from_value(serde_json::json!({
            "species": "human", "name": "test creature", "faction": "neutral", "binding": "intellect",
            "hit_dice": [{"source": "racial"}, {"source": "warrior"}]
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
        let package = super::super::Package::from_parts(manifest, regions).unwrap();
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
            .contains(&tor_simulation::grants::Ability::MagicBolt));
    }

    #[test]
    fn creature_validation_precedes_lazy_faction_diagnostics() {
        let mut manifest = creature_manifest();
        PreparedDefinitions::new_with_source(&manifest, 1, || {
            panic!("successful compilation must not acquire diagnostic source")
        })
        .unwrap();
        manifest.factions.insert("other".into(), Default::default());
        let original = manifest.characters[0].creature.as_ref().unwrap().clone();
        manifest.characters[0]
            .creature
            .as_mut()
            .unwrap()
            .hit_dice
            .clear();
        let error = PreparedDefinitions::new_with_source(&manifest, 1, || {
            panic!("invalid choices precede faction source acquisition")
        })
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
        let error = PreparedDefinitions::new_with_source(&manifest, 1, || {
            reads.set(reads.get() + 1);
            Some(source.clone())
        })
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
        manifest.characters[0].anatomy = Some(super::super::AnatomySpec { slots: vec![] });
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

        let actor: super::Actor = serde_json::from_value(serde_json::json!({"id": 2, "at": [2, 1, 0], "creature": manifest.characters[0].creature, "anatomy": {"slots": []}})).unwrap();
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
            PreparedArchetype::new(arrow, &manifest, &manifest.creatures.compile().unwrap())
                .unwrap()
                .item_asset
                .as_deref(),
            Some("item.arrow")
        );
        let hidden = &manifest.archetypes["healing"];
        assert_eq!(item_asset(hidden, &manifest), Some("item.shared"));
        assert_eq!(
            PreparedArchetype::new(hidden, &manifest, &manifest.creatures.compile().unwrap())
                .unwrap()
                .item_asset
                .as_deref(),
            Some("item.shared")
        );
        manifest.appearance_pools.get_mut("potions").unwrap().asset = None;
        let hidden = &manifest.archetypes["healing"];
        assert_eq!(item_asset(hidden, &manifest), None);
        assert_eq!(
            PreparedArchetype::new(hidden, &manifest, &manifest.creatures.compile().unwrap())
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
                memory_ticks: 73,
                flee_percent: 19,
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
            .memory_ticks = 999;
        assert_eq!(
            prepared.characters[1]
                .creature
                .control
                .profile()
                .unwrap()
                .unwrap()
                .memory_ticks,
            73
        );
        let selected_other = PreparedDefinitions::new(&manifest, 2).unwrap();
        assert!(selected_other.characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .is_none());
        assert!(selected_other.omitted_carrier(Some(1)));
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
                memory_ticks: 73,
                flee_percent: 19,
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
        manifest.creatures.species.get_mut("figure").unwrap().grants =
            vec![crate::creature_authoring::Grant::Health { amount: 33 }];
        let mut creature = definitions.characters[0].creature.clone().unwrap();
        creature.name = "guard".into();
        manifest.archetypes.insert(
            "guard".into(),
            Archetype {
                creature: Some(creature),
                anatomy: None,
                equipment: None,
                consumable: None,
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
        assert_eq!(prepared.ai_profiles["careful"].memory_ticks, 73);
        assert_eq!(prepared.ai_profiles["careful"].flee_percent, 19);
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
