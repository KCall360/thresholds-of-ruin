//! Enabled source closure for spatial capability availability, not a live loot pool.
//! Future purchases are conservative: current failed prerequisites are not proof
//! that growth/respec can never make a talent available. Imported items and
//! abilities are not roots merely because their definitions exist. Referenced
//! species/type/template modifiers remain conservative sources even when a
//! template currently replaces a type/subtype; this is source admission, not
//! an exact active-derived-modifier query.
use super::*;
use crate::creature_authoring as a;
use tor_simulation::rules as r;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Source {
    Item(String),
    Species(String),
    Template(String),
    Type(String),
    Subtype(String),
    Class(String),
    Talent(String),
    Form(String),
    Flag(String),
    Condition(String),
    Ability(String),
}
struct Sources<'a> {
    package: &'a Package,
    rules: &'a r::RulesetDocument,
    pending: Vec<Source>,
    seen: BTreeSet<Source>,
    light: bool,
}
impl Sources<'_> {
    fn add(&mut self, source: Source) {
        if self.seen.insert(source.clone()) {
            self.pending.push(source);
        }
    }
    fn grants(&mut self, grants: &[r::PointGrant]) {
        let rules = self.rules;
        // A grant-capable progression source can expose future purchases. Do
        // not invent a second requirement solver or filter on current stats.
        if grants.iter().any(|grant| {
            grant.points > 0
                && rules.point_pools.iter().any(|pool| {
                    pool.id == grant.pool && pool.allowed.contains(&r::PurchaseKind::Talent)
                })
        }) {
            for talent in &rules.talents {
                self.add(Source::Talent(talent.id.to_string()));
            }
        }
    }
    fn purchases(&mut self, purchases: &[a::Purchase]) {
        for purchase in purchases {
            match &purchase.selection {
                a::PurchaseSelection::Talent { talent } => self.add(Source::Talent(talent.clone())),
                a::PurchaseSelection::TalentForm { talent, form } => {
                    self.add(Source::Talent(talent.clone()));
                    self.add(Source::Form(form.clone()));
                }
                _ => {}
            }
        }
    }
    fn build(&mut self, build: &a::BuildSpec) {
        let rules = self.rules;
        self.add(Source::Species(build.species.clone()));
        for template in &build.templates {
            self.add(Source::Template(template.clone()));
        }
        for hd in &build.hit_dice {
            if hd.source == "racial" {
                self.grants(&rules.constants.racial_point_grants);
            } else {
                self.add(Source::Class(hd.source.clone()));
            }
            self.purchases(&hd.purchases);
        }
        match &build.future_growth {
            a::FutureGrowth::Selected {
                source: a::GrowthSource::Class { class },
            } => self.add(Source::Class(class.clone())),
            a::FutureGrowth::Selected {
                source: a::GrowthSource::Racial {},
            } => self.grants(&rules.constants.racial_point_grants),
            // Authored restoration/growth uses exactly the retained hit-die
            // plan already traversed above, not every imported class.
            a::FutureGrowth::Authored {} | a::FutureGrowth::Disabled {} => {}
        }
        if build.nutrition == tor_simulation::nutrition::NutritionRequirement::Required {
            if let tor_simulation::nutrition::NutritionConditionTrigger::OnEmptyDepletion {
                condition,
                ..
            } = &rules.nutrition_condition_trigger
            {
                self.add(Source::Condition(condition.to_string()));
            }
        }
    }
    fn author_modifiers(&mut self, values: &[a::Modifier]) {
        for value in values {
            match &value.op {
                a::Op::GrantAbility { ability } => self.add(Source::Ability(ability.clone())),
                a::Op::Flag { flag } => self.add(Source::Flag(flag.clone())),
                _ => {}
            }
        }
    }
    fn modifiers(&mut self, values: &[r::Modifier]) {
        for value in values {
            match &value.op {
                r::ModifierOp::GrantAbility { ability } => {
                    self.add(Source::Ability(ability.to_string()))
                }
                r::ModifierOp::Flag { flag } => self.add(Source::Flag(flag.to_string())),
                _ => {}
            }
        }
    }
    fn author_effects(&mut self, values: &[Effect]) {
        for value in values {
            match value {
                Effect::RoomLight | Effect::DrainLife => self.light = true,
                Effect::ApplyCondition { condition, .. }
                | Effect::ApplyPersistentCondition { condition, .. }
                | Effect::ApplyConditionForTurns { condition, .. }
                | Effect::ApplyQualifiedConditionToSource { condition, .. } => {
                    self.add(Source::Condition(condition.clone()))
                }
                _ => {}
            }
        }
    }
    fn effects(&mut self, values: &[r::Effect]) {
        for value in values {
            match value {
                r::Effect::RoomLight | r::Effect::DrainLife => self.light = true,
                r::Effect::ApplyCondition { condition, .. }
                | r::Effect::ApplyPersistentCondition { condition, .. }
                | r::Effect::ApplyConditionForTurns { condition, .. }
                | r::Effect::ApplyQualifiedConditionToSource { condition, .. } => {
                    self.add(Source::Condition(condition.to_string()))
                }
                _ => {}
            }
        }
    }
    fn finish(&mut self) -> Result<bool, Failure> {
        let rules = self.rules;
        let package = self.package;
        while let Some(source) = self.pending.pop() {
            match source {
                Source::Item(id) => {
                    let item = package
                        .manifest
                        .archetypes
                        .get(&id)
                        .ok_or_else(|| fail("Unknown spatial source item"))?;
                    if let Some(build) = &item.creature {
                        self.build(build);
                    }
                    if let Some(device) = &item.device {
                        self.add(Source::Ability(device.ability.clone()));
                    }
                    if let Some(effect) = &item.consumable {
                        self.author_effects(&effect.effects);
                    }
                    if let Some(readable) = &item.readable {
                        self.author_effects(&readable.effects);
                        if let Some(
                            tor_simulation::control_scrolls::ControlSpec::HoldNearby {
                                condition,
                                ..
                            }
                            | tor_simulation::control_scrolls::ControlSpec::ArmConfusion {
                                condition,
                                ..
                            },
                        ) = &readable.control
                        {
                            self.add(Source::Condition(condition.to_string()));
                        }
                    }
                    if let Some(scare) = &item.ground_scare {
                        self.add(Source::Condition(scare.condition.to_string()));
                    }
                    if let Some(equipment) = &item.equipment {
                        self.author_modifiers(&equipment.modifiers);
                        self.author_modifiers(&equipment.potency_modifiers);
                    }
                }
                Source::Species(id) => {
                    let species = package
                        .manifest
                        .creatures
                        .species
                        .get(&id)
                        .ok_or_else(|| fail("Unknown spatial source species"))?;
                    self.add(Source::Type(species.kind.clone()));
                    for id in &species.subtypes {
                        self.add(Source::Subtype(id.clone()));
                    }
                    self.author_modifiers(&species.modifiers);
                    for strike in &species.melee.strikes {
                        self.author_effects(&strike.on_success);
                        self.author_effects(&strike.on_failure);
                        for rider in &strike.riders {
                            self.author_effects(&rider.effects);
                        }
                    }
                }
                Source::Template(id) => {
                    let template = package
                        .manifest
                        .creatures
                        .templates
                        .get(&id)
                        .ok_or_else(|| fail("Unknown spatial source template"))?;
                    if let Some(kind) = &template.kind {
                        self.add(Source::Type(kind.clone()));
                    }
                    for id in &template.add_subtypes {
                        self.add(Source::Subtype(id.clone()));
                    }
                    self.author_modifiers(&template.modifiers);
                }
                Source::Type(id) => {
                    let value = rules
                        .creature_types
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source type"))?;
                    self.modifiers(&value.modifiers);
                }
                Source::Subtype(id) => {
                    let value = rules
                        .subtypes
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source subtype"))?;
                    self.modifiers(&value.modifiers);
                }
                Source::Class(id) => {
                    let value = rules
                        .classes
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source class"))?;
                    self.modifiers(&value.modifiers);
                    self.grants(&value.point_grants);
                }
                Source::Talent(id) => {
                    let value = rules
                        .talents
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source talent"))?;
                    self.modifiers(&value.modifiers);
                    for form in &value.form_choices {
                        self.add(Source::Form(form.to_string()));
                    }
                }
                Source::Form(id) => {
                    let value = rules
                        .forms
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source form"))?;
                    self.add(Source::Ability(value.ability.to_string()));
                }
                Source::Flag(id) => {
                    let value = rules
                        .flags
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source flag"))?;
                    self.modifiers(&value.modifiers);
                }
                Source::Condition(id) => {
                    let value = rules
                        .conditions
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source condition"))?;
                    self.modifiers(&value.modifiers);
                }
                Source::Ability(id) => {
                    let value = rules
                        .abilities
                        .iter()
                        .find(|x| x.id.as_str() == id)
                        .ok_or_else(|| fail("Unknown spatial source ability"))?;
                    match &value.strikes {
                        r::StrikeSource::Authored { strikes } => {
                            for strike in strikes {
                                self.effects(&strike.on_success);
                                self.effects(&strike.on_failure);
                                for rider in &strike.riders {
                                    self.effects(&rider.effects);
                                }
                            }
                        }
                        r::StrikeSource::NaturalMelee { additional_riders } => {
                            for rider in additional_riders {
                                self.effects(&rider.effects);
                            }
                        }
                        r::StrikeSource::SelectedMelee | r::StrikeSource::Projectile { .. } => {}
                    }
                }
            }
            if self.light {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

pub(super) fn spatial_effect_enabled(package: &Package) -> Result<bool, Failure> {
    let rules = package.rules()?;
    // Absence is an optimization only: check every current effect-owning
    // definition category before skipping the real source traversal. In
    // particular, untouched nonspatial saves retain their lazy source reads.
    let rule_light = rules
        .document()
        .abilities
        .iter()
        .any(|ability| match &ability.strikes {
            r::StrikeSource::Authored { strikes } => strikes.iter().any(|strike| {
                strike
                    .on_success
                    .iter()
                    .chain(&strike.on_failure)
                    .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                    .any(|effect| matches!(effect, r::Effect::RoomLight | r::Effect::DrainLife))
            }),
            r::StrikeSource::NaturalMelee { additional_riders } => additional_riders
                .iter()
                .flat_map(|rider| &rider.effects)
                .any(|effect| matches!(effect, r::Effect::RoomLight | r::Effect::DrainLife)),
            r::StrikeSource::SelectedMelee | r::StrikeSource::Projectile { .. } => false,
        });
    let item_light = package.manifest.archetypes.values().any(|item| {
        item.consumable
            .iter()
            .flat_map(|value| &value.effects)
            .chain(item.readable.iter().flat_map(|value| &value.effects))
            .any(|effect| matches!(effect, Effect::RoomLight | Effect::DrainLife))
    });
    let natural_light = package.manifest.creatures.species.values().any(|species| {
        species.melee.strikes.iter().any(|strike| {
            strike
                .on_success
                .iter()
                .chain(&strike.on_failure)
                .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                .any(|effect| matches!(effect, Effect::RoomLight | Effect::DrainLife))
        })
    });
    if !rule_light && !item_light && !natural_light {
        return Ok(false);
    }
    let mut sources = Sources {
        package,
        rules: rules.document(),
        pending: Vec::new(),
        seen: BTreeSet::new(),
        light: false,
    };
    for ability in &rules.document().constants.default_abilities {
        sources.add(Source::Ability(ability.to_string()));
    }
    for character in &package.manifest.characters {
        if let Some(build) = &character.creature {
            sources.build(build);
        }
        for item in &character.loadout {
            sources.add(Source::Item(item.archetype.clone()));
        }
    }
    if let Some(retrieval) = &package.manifest.unique_retrieval {
        sources.add(Source::Item(retrieval.archetype.clone()));
    }
    for entry in &package.index.regions {
        let region = package.region_def(entry.id)?;
        for item in &region.items {
            if let Some(id) = &item.archetype {
                sources.add(Source::Item(id.clone()));
            }
        }
        for actor in &region.actors {
            if let Some(id) = &actor.archetype {
                sources.add(Source::Item(id.clone()));
            }
            if let Some(build) = &actor.creature {
                sources.build(build);
            }
        }
        if let Some(generate) = &region.generate {
            for id in generate
                .actors
                .iter()
                .flat_map(|p| &p.archetypes)
                .chain(generate.items.iter().flat_map(|p| &p.archetypes))
            {
                sources.add(Source::Item(id.clone()));
            }
        }
        for trap in &region.traps {
            match &trap.kind {
                TrapKind::BearTrap { condition, .. } | TrapKind::SleepingGas { condition, .. } => {
                    sources.add(Source::Condition(condition.clone()))
                }
                TrapKind::Arrow { ammunition, .. } => sources.add(Source::Item(ammunition.clone())),
                _ => {}
            }
        }
    }
    // Only recipes referenced by real groups/families can supply trap grants.
    let recipes: BTreeSet<_> = package
        .manifest
        .generation_groups
        .values()
        .map(|g| &g.recipe)
        .chain(
            package
                .manifest
                .procedural_worlds
                .values()
                .map(|f| &f.recipe),
        )
        .collect();
    for id in recipes {
        let recipe = package
            .manifest
            .generation_recipes
            .get(id)
            .ok_or_else(|| fail("Unknown spatial source recipe"))?;
        for stage in &recipe.stages {
            if let crate::generation_recipe::Operation::HiddenTraps { config } = &stage.operation {
                sources.add(Source::Condition(config.sleep_condition.clone()));
                if let Some(hazards) = &config.hazards {
                    sources.add(Source::Condition(hazards.bear_condition.clone()));
                    sources.add(Source::Item(hazards.arrow_ammunition.clone()));
                }
            }
        }
    }
    if let Some(plan) = &package.manifest.floor_activation {
        for profile in plan.families.values() {
            let profile = plan
                .profiles
                .get(profile)
                .ok_or_else(|| fail("Unknown spatial source population profile"))?;
            for stage in &profile.stages {
                match stage {
                    PopulationStage::Items { bands, .. }
                    | PopulationStage::RoomItems { bands, .. } => {
                        for choice in bands.iter().flat_map(|b| &b.choices) {
                            sources.add(Source::Item(choice.archetype.clone()));
                        }
                    }
                    PopulationStage::Actors { bands, .. } => {
                        for choice in bands.iter().flat_map(|b| &b.choices) {
                            sources.add(Source::Item(choice.archetype.clone()));
                            for advancement in &choice.advancement {
                                sources.purchases(&advancement.purchases);
                            }
                            if let Some(loot) = &choice.carried_loot {
                                let pool = plan
                                    .carried_item_pools
                                    .get(&loot.pool)
                                    .ok_or_else(|| fail("Unknown spatial source carried pool"))?;
                                for item in &pool.choices {
                                    sources.add(Source::Item(item.archetype.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    sources.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_simulation::content_ids::AbilityId;

    fn fixture() -> Package {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
        let scenario = load(&root, 7, None, true).unwrap();
        let mut package = (**scenario.package.as_ref().unwrap()).clone();
        let mut document = package.rules().unwrap().document().clone();
        let mut light = document
            .abilities
            .iter()
            .find(|ability| ability.id.as_str() == "rogue.wand_nothing")
            .unwrap()
            .clone();
        light.id = AbilityId::new("fixture.room_light").unwrap();
        let r::StrikeSource::Authored { strikes } = &mut light.strikes else {
            panic!("production self-device prerequisite")
        };
        assert_eq!(strikes.len(), 1);
        assert!(strikes[0].on_success.is_empty());
        strikes[0].on_success.push(r::Effect::RoomLight);
        document.abilities.push(light);
        package.rules_document =
            crate::storage::schema::scenario::encode_rules_document(&document).unwrap();
        let rules = package.rules().unwrap(); // Actual complete rules validator.
        let mut item = package.manifest.archetypes["rogue.wand_nothing"].clone();
        item.identity = Some("fixture.room_light".into());
        item.device.as_mut().unwrap().ability = "fixture.room_light".into();
        item.device.as_ref().unwrap().compile(&rules).unwrap();
        // No appearance pool: this typed source-closure fixture is not adding
        // an unlicensed tenth identity to the production cosmetic pool.
        item.appearance_pool = None;
        package
            .manifest
            .archetypes
            .insert("fixture.room_light".into(), item);
        package
    }

    #[test]
    fn unused_import_is_not_enabled_but_actual_starting_device_is() {
        let mut package = fixture();
        assert!(!spatial_effect_enabled(&package).unwrap());
        package.manifest.characters[0]
            .loadout
            .push(CharacterLoadout {
                archetype: "fixture.room_light".into(),
                quantity: 1,
                equipped_slot: None,
            });
        assert!(spatial_effect_enabled(&package).unwrap());
    }

    #[test]
    fn reached_species_grant_requires_metadata_without_stock_identity_inference() {
        let mut package = fixture();
        assert!(!spatial_effect_enabled(&package).unwrap());
        let species = package.manifest.characters[0]
            .creature
            .as_ref()
            .unwrap()
            .species
            .clone();
        package
            .manifest
            .creatures
            .species
            .get_mut(&species)
            .unwrap()
            .modifiers
            .push(a::Modifier {
                scope: a::Scope::Always,
                op: a::Op::GrantAbility {
                    ability: "fixture.room_light".into(),
                },
            });
        assert!(spatial_effect_enabled(&package).unwrap());
    }

    #[test]
    fn available_equipment_and_condition_flag_chain_reach_the_same_capability() {
        let mut package = fixture();
        let rules = package.rules().unwrap();
        let mut equipment = package
            .manifest
            .archetypes
            .values()
            .find(|item| item.equipment.is_some())
            .unwrap()
            .clone();
        equipment.appearance_pool = None;
        equipment.identity = Some("fixture.light_equipment".into());
        equipment
            .equipment
            .as_mut()
            .unwrap()
            .modifiers
            .push(a::Modifier {
                scope: a::Scope::Always,
                op: a::Op::GrantAbility {
                    ability: "fixture.room_light".into(),
                },
            });
        equipment
            .equipment
            .as_ref()
            .unwrap()
            .compile(&rules)
            .unwrap();
        package
            .manifest
            .archetypes
            .insert("fixture.light_equipment".into(), equipment);
        assert!(!spatial_effect_enabled(&package).unwrap());
        package.manifest.characters[0]
            .loadout
            .push(CharacterLoadout {
                archetype: "fixture.light_equipment".into(),
                quantity: 1,
                equipped_slot: None,
            });
        assert!(spatial_effect_enabled(&package).unwrap());

        let mut document = rules.document().clone();
        let flag = tor_simulation::content_ids::FlagId::new("fixture.light_grant").unwrap();
        document.flags.push(r::FlagDef {
            id: flag.clone(),
            name: "Light grant".into(),
            modifiers: vec![r::Modifier {
                scope: r::Scope::Always,
                op: r::ModifierOp::GrantAbility {
                    ability: AbilityId::new("fixture.room_light").unwrap(),
                },
            }],
        });
        let condition = document.conditions.first_mut().unwrap();
        let condition_id = condition.id.to_string();
        condition.modifiers.push(r::Modifier {
            scope: r::Scope::Always,
            op: r::ModifierOp::Flag { flag },
        });
        r::Ruleset::validate(document.clone()).unwrap();
        let mut graph = Sources {
            package: &package,
            rules: &document,
            pending: vec![],
            seen: BTreeSet::new(),
            light: false,
        };
        assert!(!graph.finish().unwrap());
        graph.add(Source::Condition(condition_id));
        assert!(graph.finish().unwrap());
    }

    #[test]
    fn future_talent_pool_is_conservative_but_unused_ability_is_not_a_root() {
        let package = fixture();
        let rules = package.rules().unwrap();
        let mut document = rules.document().clone();
        let talent = document
            .talents
            .first_mut()
            .expect("production talent control");
        talent.modifiers.push(r::Modifier {
            scope: r::Scope::Always,
            op: r::ModifierOp::GrantAbility {
                ability: AbilityId::new("fixture.room_light").unwrap(),
            },
        });
        r::Ruleset::validate(document.clone()).unwrap();
        let grants = document.constants.racial_point_grants.clone();
        assert!(
            grants.iter().any(|grant| grant.points > 0
                && document
                    .point_pools
                    .iter()
                    .any(|pool| pool.id == grant.pool
                        && pool.allowed.contains(&r::PurchaseKind::Talent))),
            "real future talent pool prerequisite"
        );
        let mut graph = Sources {
            package: &package,
            rules: &document,
            pending: vec![],
            seen: BTreeSet::new(),
            light: false,
        };
        assert!(!graph.finish().unwrap());
        graph.grants(&grants);
        assert!(graph.finish().unwrap());
    }
}
