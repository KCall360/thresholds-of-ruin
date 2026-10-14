//! Bounded authoring adapters for once-only entry population and initial kits.
//! These declarations describe immutable content. Only simulation evaluates entry.
use super::*;
use tor_simulation::floor_activation as runtime;

const MAX_PROFILES: usize = runtime::MAX_PROFILES;
const MAX_STAGES: usize = runtime::MAX_STAGES;
const MAX_CHOICES: usize = runtime::MAX_CHOICES;
const MAX_ATTEMPTS: u16 = runtime::MAX_ATTEMPTS;
const MAX_LOADOUT: usize = 64;

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PopulationProfile {
    pub version: u32,
    pub stages: Vec<PopulationStage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PopulationStage {
    Items {
        id: String,
        version: u32,
        minimum_depth: u32,
        count: [u16; 2],
        archetypes: Vec<String>,
        #[serde(default = "unit_quantity")]
        quantity: u64,
    },
    Actors {
        id: String,
        version: u32,
        minimum_depth: u32,
        count: [u16; 2],
        archetypes: Vec<String>,
        ai: Option<String>,
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
    fn header(&self) -> (&str, u32, u32, [u16; 2], &[String]) {
        match self {
            Self::Items {
                id,
                version,
                minimum_depth,
                count,
                archetypes,
                ..
            }
            | Self::Actors {
                id,
                version,
                minimum_depth,
                count,
                archetypes,
                ..
            } => (id, *version, *minimum_depth, *count, archetypes),
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
        for (name, profile) in &plan.profiles {
            require(
                label(name)
                    && profile.version == 1
                    && profile.stages.len() <= MAX_STAGES
                    && profile
                        .stages
                        .iter()
                        .map(|stage| u32::from(stage.header().3[1]))
                        .sum::<u32>()
                        <= u32::from(MAX_ATTEMPTS),
                "Invalid population profile or unsupported version",
            )?;
            let mut ids = BTreeSet::new();
            for stage in &profile.stages {
                let (id, version, minimum, count, choices) = stage.header();
                require(
                    label(id)
                        && ids.insert(id)
                        && version == 1
                        && minimum > 0
                        && count[0] <= count[1]
                        && count[1] <= MAX_ATTEMPTS
                        && !choices.is_empty()
                        && choices.len() <= MAX_CHOICES,
                    "Invalid population stage",
                )?;
                let unique: BTreeSet<_> = choices.iter().collect();
                require(unique.len() == choices.len(), "Duplicate population choice")?;
                for choice in choices {
                    self.author_archetype(choice)?;
                }
                match stage {
                    PopulationStage::Items { quantity, .. } => {
                        require(*quantity > 0, "Empty population item stack")?
                    }
                    PopulationStage::Actors { ai: Some(ai), .. } => require(
                        self.manifest.ai_profiles.contains_key(ai),
                        "Unknown population AI profile",
                    )?,
                    PopulationStage::Actors { .. } => (),
                }
            }
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
                        let (id, version, minimum_depth, count, archetypes) = stage.header();
                        let population = match stage {
                            PopulationStage::Items { quantity, .. } => {
                                runtime::PopulationContent::Items {
                                    choices: archetypes
                                        .iter()
                                        .map(|archetype| {
                                            Ok(runtime::PreparedItemChoice {
                                                content: archetype.clone(),
                                                quantity: *quantity,
                                                spec: index.definitions.population_item(
                                                    archetype,
                                                    *quantity,
                                                    index.seed,
                                                    &index.appearances,
                                                )?,
                                            })
                                        })
                                        .collect::<Result<_, Failure>>()?,
                                }
                            }
                            PopulationStage::Actors {
                                ai,
                                avoid_protagonist_sight,
                                ..
                            } => runtime::PopulationContent::Actors {
                                choices: archetypes
                                    .iter()
                                    .map(|archetype| {
                                        index.definitions.population_actor(archetype, ai.as_deref())
                                    })
                                    .collect::<Result<_, _>>()?,
                                avoid_protagonist_sight: *avoid_protagonist_sight,
                            },
                        };
                        Ok(runtime::PopulationStage {
                            id: id.into(),
                            version,
                            minimum_depth,
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
        Ok(Some(runtime::PopulationPlan {
            protagonist: tor_simulation::ActorId(self.selected),
            profiles,
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
            return game
                .activate_initial_floor()
                .map_err(|error| fail(format!("Initial floor activation: {error:?}")));
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
        game.activate_initial_floor()
            .map_err(|error| fail(format!("Initial floor activation: {error:?}")))
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
        record.profile == scope.profile
            && record
                .decisions
                .iter()
                .all(|decision| match &decision.result {
                    runtime::StageResult::Skipped { .. } => true,
                    runtime::StageResult::Evaluated { attempts, .. } => {
                        attempts.iter().all(|attempt| match attempt {
                            runtime::PlacementDecision::Skipped { .. } => true,
                            runtime::PlacementDecision::Placed {
                                location, entity, ..
                            } => {
                                let population_identity = match entity {
                                    runtime::PopulationEntity::Actor { id } => {
                                        id.0 >= self.actor_floor
                                    }
                                    runtime::PopulationEntity::Item { id } => {
                                        id.0 >= self.item_floor
                                    }
                                };
                                population_identity
                                    && scope.cells.binary_search(location).is_ok()
                                    && !scope.protected.contains(location)
                            }
                        })
                    }
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
            let definitions = self.package.group_definitions(identity, self.seed).ok()?;
            let members: Vec<_> = definitions.values().collect();
            self.package
                .activation_scope(identity, depth, &members)
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
            {"kind":"items","id":"stock","version":1,"minimum_depth":1,"count":[1,1],"archetypes":["marker"]}]}}});
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
            serde_json::from_value(value).unwrap(),
            vec![],
            &package.rules_source,
        )
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
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["archetypes"] =
                        json!(["marker", "marker"])
                }
                9 => {
                    value["floor_activation"]["profiles"]["plain"]["stages"][0]["archetypes"] =
                        json!(["unknown"])
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
        for stage in [
            json!({"kind":"script","id":"stock","version":1,"minimum_depth":1,"count":[1,1],"archetypes":["marker"]}),
            json!({"kind":"items","id":"stock","version":1,"minimum_depth":1,"count":[1,1],"archetypes":["marker"],"ai":"wander"}),
            json!({"kind":"actors","id":"foe","version":1,"minimum_depth":1,"count":[1,1],"archetypes":["marker"],"quantity":1}),
            json!({"kind":"items","id":"stock","version":1,"minimum_depth":1,"count":[1,1],"archetypes":["marker"],"when":"inventory.ration == 0"}),
        ] {
            assert!(serde_json::from_value::<PopulationStage>(stage).is_err());
        }
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
            entity: runtime::PopulationEntity::Item { id },
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
            };
            assert!(
                !configuration.valid_record(&record),
                "authored or selected-kit identity {reserved} is not a population origin"
            );
        }
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
