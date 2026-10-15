//! Narrow author-owned retrieval policy. The ordinary item store owns identity
//! and location; this adapter only pins immutable source policy and bounds.
use super::*;
use tor_simulation::unique_retrieval as runtime;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniqueRetrieval {
    pub version: u32,
    pub protagonist: Protagonist,
    pub family: String,
    pub minimum_depth: u32,
    pub archetype: String,
    pub exit_anchor: String,
}

impl Package {
    pub(super) fn check_unique_retrieval(&self) -> Result<(), Failure> {
        let Some(policy) = &self.manifest.unique_retrieval else {
            return Ok(());
        };
        let protagonist = match &policy.protagonist {
            Protagonist::Actor(actor) => *actor,
            Protagonist::Selected(selector) => {
                require(
                    selector == "selected_character",
                    "Unknown retrieval protagonist selector",
                )?;
                self.selected
            }
        };
        require(
            policy.version == 1 && policy.minimum_depth == 26,
            "Unsupported unique retrieval version or depth",
        )?;
        require(
            protagonist == self.selected
                && self
                    .manifest
                    .characters
                    .iter()
                    .any(|actor| actor.id == protagonist),
            "Retrieval protagonist must be the selected declared character",
        )?;
        require(
            self.manifest.objective.is_none() && self.manifest.arena.is_none(),
            "Unique retrieval cannot combine with another run objective",
        )?;
        require(
            self.manifest.procedural_worlds.contains_key(&policy.family)
                && self
                    .manifest
                    .floor_activation
                    .as_ref()
                    .is_some_and(|plan| plan.families.contains_key(&policy.family)),
            "Unique retrieval requires an entry-populated procedural family",
        )?;
        let family = &self.manifest.procedural_worlds[&policy.family];
        let minimum_limit = match family.domain {
            crate::procedural_world::Domain::SeededFinite { min, .. } => min,
            _ => family.max_depth(),
        };
        require(
            minimum_limit >= policy.minimum_depth,
            "Unique retrieval family must reach its objective depth for every seed",
        )?;
        require(
            policy.exit_anchor == format!("{}/1/up", policy.family),
            "Unique retrieval must exit through the family's floor-one up endpoint",
        )?;
        let artifact = self
            .manifest
            .archetypes
            .get(&policy.archetype)
            .ok_or_else(|| fail("Missing retrieval archetype"))?;
        require(
            artifact.class == ItemClass::Amulet
                && !artifact.stackable
                && artifact.creature.is_none()
                && artifact.equipment.is_none()
                && artifact.consumable.is_none()
                && artifact.edible.is_none()
                && artifact.supply.is_none()
                && artifact.appearance_pool.is_none(),
            "Unique retrieval needs a plain, nonstackable Amulet definition",
        )?;
        let identity = artifact.identity.as_ref().unwrap_or(&policy.archetype);
        require(
            self.manifest.archetypes.iter().all(|(name, definition)| {
                name == &policy.archetype
                    || definition.identity.as_ref().unwrap_or(name) != identity
            }),
            "Reserved retrieval identity has another archetype",
        )?;
        require(
            self.manifest
                .characters
                .iter()
                .flat_map(|actor| &actor.loadout)
                .all(|row| row.archetype != policy.archetype),
            "Reserved retrieval item cannot appear in a starting kit",
        )?;
        let plan = self
            .manifest
            .floor_activation
            .as_ref()
            .expect("checked plan");
        for profile in plan.profiles.values() {
            for stage in &profile.stages {
                if let PopulationStage::Items { bands, .. } = stage {
                    require(
                        bands
                            .iter()
                            .flat_map(|band| &band.choices)
                            .all(|choice| choice.archetype != policy.archetype),
                        "Reserved retrieval item cannot appear in floor population",
                    )?;
                }
            }
        }
        // Every pool is source policy, even unused or selected with zero chance.
        require(
            plan.carried_item_pools
                .values()
                .flat_map(|pool| &pool.choices)
                .all(|choice| choice.archetype != policy.archetype),
            "Reserved retrieval item cannot appear in carried stock",
        )?;
        Ok(())
    }

    pub(super) fn check_retrieval_region(&self, region: &RegionDef) -> Result<(), Failure> {
        let Some(policy) = &self.manifest.unique_retrieval else {
            return Ok(());
        };
        let definition = self
            .manifest
            .archetypes
            .get(&policy.archetype)
            .ok_or_else(|| fail("Missing retrieval archetype"))?;
        let identity = definition.identity.as_ref().unwrap_or(&policy.archetype);
        require(
            region.items.iter().all(|item| {
                item.archetype.as_ref() != Some(&policy.archetype)
                    && (item.archetype.is_some()
                        || item
                            .name
                            .iter()
                            .chain(&item.seed_names)
                            .all(|name| name != &policy.archetype && name != identity))
            }) && region
                .generate
                .as_ref()
                .and_then(|generator| generator.items.as_ref())
                .is_none_or(|pool| !pool.archetypes.contains(&policy.archetype)),
            "Reserved retrieval item cannot appear in authored instances or generation pools",
        )
    }

    pub(super) fn retrieval_policy(
        &self,
        index: &PackageIndex,
    ) -> Result<Option<runtime::UniqueRetrievalPolicy>, Failure> {
        let Some(policy) = &self.manifest.unique_retrieval else {
            return Ok(None);
        };
        let mut anchors = index.anchors.clone();
        let at =
            self.character_anchor(self.selected, index.seed, &policy.exit_anchor, &mut anchors)?;
        Ok(Some(runtime::UniqueRetrievalPolicy {
            version: policy.version,
            protagonist: tor_simulation::ActorId(self.selected),
            family: policy.family.clone(),
            minimum_depth: policy.minimum_depth,
            item: index.definitions.population_item(
                &policy.archetype,
                1,
                index.seed,
                &index.appearances,
            )?,
            exit: runtime::TerminalExit {
                at,
                direction: Direction::Up,
            },
        }))
    }

    pub(crate) fn retrieval_configuration(
        &self,
        seed: u64,
    ) -> Result<RetrievalConfiguration<'_>, Failure> {
        let (policy, item_floor) = if self.manifest.unique_retrieval.is_some() {
            let index = self.index(seed)?;
            let character = self
                .manifest
                .characters
                .iter()
                .find(|actor| actor.id == self.selected)
                .ok_or_else(|| fail("Missing selected retrieval character"))?;
            let floor = index
                .ceilings
                .1
                .checked_add(character.loadout.len() as u64)
                .ok_or_else(|| fail("Retrieval item identity floor overflow"))?;
            (self.retrieval_policy(&index)?, floor)
        } else {
            (None, 1)
        };
        Ok(RetrievalConfiguration {
            package: self,
            seed,
            policy,
            item_floor,
        })
    }
}

pub(crate) struct RetrievalConfiguration<'a> {
    package: &'a Package,
    seed: u64,
    policy: Option<runtime::UniqueRetrievalPolicy>,
    item_floor: u64,
}

impl RetrievalConfiguration<'_> {
    pub(crate) fn valid_game(&self, game: &Game) -> bool {
        game.seed() == self.seed && game.retrieval_configuration_matches(self.policy.as_ref())
    }

    pub(crate) fn valid_record(&self, entry: &runtime::RetrievalEntry) -> bool {
        let Some(policy) = &self.policy else {
            return false;
        };
        if entry.entrant != policy.protagonist
            || entry.depth < policy.minimum_depth
            || entry.scope != format!("{}/{}", policy.family, entry.depth)
        {
            return false;
        }
        let Some(family) = self.package.manifest.procedural_worlds.get(&policy.family) else {
            return false;
        };
        let destination = |at: Location| {
            family
                .address(at.region.0)
                .is_some_and(|(depth, _)| depth == entry.depth)
                && at.position.z == 0
                && self.valid_location(at)
        };
        match &entry.result {
            runtime::RetrievalResult::Created { item, at, .. } => {
                item.0 >= self.item_floor && destination(*at)
            }
            runtime::RetrievalResult::Relocated { from, at, .. } => {
                destination(*at)
                    && match from {
                        runtime::RetrievalSource::Ground(source) => self.valid_location(*source),
                        runtime::RetrievalSource::Carried(actor) => actor.0 > 0,
                    }
            }
            runtime::RetrievalResult::AlreadyCarried { .. } => true,
        }
    }

    // Historical source bounds belong to their own source-pinned address. They
    // prove structure, not the mutable walkability of an expired old snapshot.
    fn valid_location(&self, at: Location) -> bool {
        self.package
            .structural_definition(self.seed, at.region.0)
            .is_ok_and(|region| {
                let p = at.position;
                p.x >= 0
                    && p.y >= 0
                    && p.z >= 0
                    && p.x < region.size[0]
                    && p.y < region.size[1]
                    && p.z < region.size[2]
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reserved_retrieval_definition_rejects_authored_and_zero_count_generator_births() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
        let original = resolve_source(&root).unwrap();
        let mut manifest = original.manifest.clone();
        manifest.unique_retrieval = Some(
            serde_json::from_value(json!({
                "version":1,"protagonist":"selected_character","family":"dungeon",
                "minimum_depth":26,"archetype":"amulet","exit_anchor":"dungeon/1/up"
            }))
            .unwrap(),
        );
        manifest.archetypes.insert(
            "amulet".into(),
            serde_json::from_value(json!({
                "name":"Amulet of Yendor","identity":"amulet-of-yendor","class":"amulet"
            }))
            .unwrap(),
        );
        manifest.archetypes.insert(
            "ordinary".into(),
            serde_json::from_value(json!({
                "name":"Amulet of Yendor","identity":"ordinary-lookalike","class":"amulet"
            }))
            .unwrap(),
        );
        let package = Package::from_parts(
            manifest,
            vec![],
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap();
        let base = json!({"id":999,"name":"unused","size":[7,5,2]});
        for change in ["archetype", "name", "seed_name", "zero_count_pool"] {
            let mut region = base.clone();
            if change == "zero_count_pool" {
                region["generate"] = json!({"generator":"rooms","version":2,
                    "rooms":[1,1],"items":{"count":[0,0],"archetypes":["amulet"]}});
            } else {
                region["items"] = json!([{"id":100,"at":[1,1,0]}]);
                match change {
                    "archetype" => region["items"][0]["archetype"] = json!("amulet"),
                    "name" => region["items"][0]["name"] = json!("amulet-of-yendor"),
                    "seed_name" => region["items"][0]["seed_names"] = json!(["other", "amulet"]),
                    _ => unreachable!(),
                }
            }
            let definition: RegionDef = serde_json::from_value(region).unwrap();
            assert!(
                package.check_retrieval_region(&definition).is_err(),
                "{change}"
            );
        }
        let mut lookalike = base;
        lookalike["items"] = json!([{"id":100,"at":[1,1,0],"archetype":"ordinary"},
            {"id":101,"at":[2,1,0],"name":"Amulet of Yendor"}]);
        package
            .check_retrieval_region(&serde_json::from_value(lookalike).unwrap())
            .unwrap();
    }
}
