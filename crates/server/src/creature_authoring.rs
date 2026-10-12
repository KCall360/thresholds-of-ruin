//! Independent author-owned creature declarations. Content names are resolved
//! against the scenario's validated ruleset; no catalog is mirrored in code.
use crate::{
    scenario_package::{AnatomySpec, DamageType},
    Failure,
};
use s::{attributes::ByAttribute, content_ids::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tor_simulation as s;

fn invalid(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

pub type Attribute = String;
pub type Skill = String;
pub type ManaBinding = String;
pub type HitDieSource = String;
pub type Talent = String;

fn primitive<T: serde::de::DeserializeOwned>(value: &str, category: &str) -> Result<T, Failure> {
    serde_json::from_value(serde_json::Value::String(value.into()))
        .map_err(|_| invalid(format!("Unknown {category} {value:?}")))
}

pub(crate) fn hit_die_source(
    value: &str,
    rules: &s::rules::Ruleset,
) -> Result<s::progression::HdSource, Failure> {
    if value == "racial" {
        return Ok(s::progression::HdSource::Racial);
    }
    let id = ClassId::new(value).map_err(|_| invalid(format!("Invalid class ID {value:?}")))?;
    if rules.class(&id).is_none() {
        return Err(invalid(format!("Unknown class {value:?}")));
    }
    Ok(s::progression::HdSource::Class(id))
}
pub(crate) fn skill(value: &str) -> Result<s::attributes::Skill, Failure> {
    primitive(value, "skill")
}
pub(crate) fn attribute(value: &str) -> Result<s::attributes::Attribute, Failure> {
    primitive(value, "attribute")
}
pub(crate) fn talent(value: &str, rules: &s::rules::Ruleset) -> Result<TalentId, Failure> {
    let id = TalentId::new(value).map_err(|_| invalid(format!("Invalid talent ID {value:?}")))?;
    if rules.talent(&id).is_none() {
        return Err(invalid(format!("Unknown talent {value:?}")));
    }
    Ok(id)
}

// Generic mechanism DTOs deliberately contain authored text, not interned IDs
// or persisted actor records. Their vocabulary is independent of game content.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modifier {
    pub scope: Scope,
    pub op: Op,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Always,
    VersusSource,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Add { stat: Stat, value: i64 },
    Set { stat: Stat, value: i64 },
    Flag { flag: String },
    Immunity { selector: Selector },
    Reduction { selector: Selector, amount: u32 },
    GrantAbility { ability: String },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Stat {
    Attribute { attribute: String },
    Defense { defense: String },
    Health,
    ResourceMax { resource: String },
    Ability { ability: String, stat: String },
    AbilityTag { tag: String, stat: String },
    Edge,
    CheckEdge,
    IncomingEdge { defense: Option<String> },
}

impl<'de> Deserialize<'de> for Stat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Serde's internally tagged unit variants ignore extra fields. Empty
        // struct variants enforce the strict authoring boundary without changing
        // the public enum constructors or canonical serialized representation.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Recorded {
            Attribute { attribute: String },
            Defense { defense: String },
            Health {},
            ResourceMax { resource: String },
            Ability { ability: String, stat: String },
            AbilityTag { tag: String, stat: String },
            Edge {},
            CheckEdge {},
            IncomingEdge { defense: Option<String> },
        }
        Ok(match Recorded::deserialize(deserializer)? {
            Recorded::Attribute { attribute } => Self::Attribute { attribute },
            Recorded::Defense { defense } => Self::Defense { defense },
            Recorded::Health {} => Self::Health,
            Recorded::ResourceMax { resource } => Self::ResourceMax { resource },
            Recorded::Ability { ability, stat } => Self::Ability { ability, stat },
            Recorded::AbilityTag { tag, stat } => Self::AbilityTag { tag, stat },
            Recorded::Edge {} => Self::Edge,
            Recorded::CheckEdge {} => Self::CheckEdge,
            Recorded::IncomingEdge { defense } => Self::IncomingEdge { defense },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Category { category: DamageType },
    Descriptor { descriptor: String },
}
impl Modifier {
    pub(crate) fn compile(&self) -> Result<s::rules::Modifier, Failure> {
        let value = serde_json::to_value(self).map_err(|_| invalid("Invalid modifier"))?;
        serde_json::from_value(value).map_err(|error| invalid(format!("Invalid modifier: {error}")))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Melee {
    pub skill: String,
    pub bonus: i32,
    pub wind_up: u64,
    pub recovery: u64,
    pub damage: Damage,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Damage {
    pub components: Vec<Component>,
    pub primary: Option<DamageKey>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageKey {
    pub category: DamageType,
    pub descriptor: Option<String>,
    pub sides: Option<u16>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub category: DamageType,
    pub descriptor: Option<String>,
    pub amount: Amount,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Amount {
    Fixed { value: u32 },
    Rolled { count: u16, sides: u16, bonus: i32 },
}
fn descriptor(
    value: &Option<String>,
    rules: &s::rules::Ruleset,
) -> Result<Option<DescriptorId>, Failure> {
    value
        .as_ref()
        .map(|value| {
            let id = DescriptorId::new(value)
                .map_err(|_| invalid(format!("Invalid descriptor {value:?}")))?;
            if !rules
                .document()
                .descriptors
                .iter()
                .any(|definition| definition.id == id)
            {
                return Err(invalid(format!("Unknown descriptor {value:?}")));
            }
            Ok(id)
        })
        .transpose()
}
impl Melee {
    pub(crate) fn compile(
        &self,
        rules: &s::rules::Ruleset,
    ) -> Result<s::attacks::MeleeAttack, Failure> {
        if self.damage.components.len() > 32 {
            return Err(invalid("Too many damage components"));
        }
        let components = self
            .damage
            .components
            .iter()
            .map(|component| {
                let descriptor = descriptor(&component.descriptor, rules)?;
                Ok(match component.amount {
                    Amount::Fixed { value } => s::damage::DamageComponent::fixed(
                        component.category.into(),
                        descriptor,
                        value,
                    ),
                    Amount::Rolled {
                        count,
                        sides,
                        bonus,
                    } => s::damage::DamageComponent::rolled(
                        component.category.into(),
                        descriptor,
                        s::dice::DicePool::new(count, sides, bonus)
                            .map_err(|_| invalid("Invalid damage dice"))?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let primary = self
            .damage
            .primary
            .as_ref()
            .map(|key| {
                Ok::<_, Failure>(s::damage::DamageKey {
                    category: key.category.into(),
                    descriptor: descriptor(&key.descriptor, rules)?,
                    sides: key.sides,
                })
            })
            .transpose()?;
        let damage = s::damage::DamageSpec::new(components, primary)
            .map_err(|_| invalid("Invalid damage definition"))?;
        s::attacks::MeleeAttack::new(
            skill(&self.skill)?,
            self.bonus,
            self.wind_up,
            self.recovery,
            damage,
        )
        .map_err(|_| invalid("Invalid natural attack"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Species {
    pub kind: String,
    #[serde(default)]
    pub subtypes: Vec<String>,
    pub attributes: ByAttribute<u16>,
    pub anatomy: Option<AnatomySpec>,
    pub melee: Melee,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}
// Template stages may omit unchanged attributes; base attributes may not.
fn partial_attributes<'de, D, T>(deserializer: D) -> Result<ByAttribute<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    struct Visitor<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de> + Default> serde::de::Visitor<'de> for Visitor<T> {
        type Value = ByAttribute<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("named attribute adjustments")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut result = ByAttribute::<T>::default();
            let mut seen = BTreeSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !seen.insert(key.clone()) {
                    return Err(serde::de::Error::custom("duplicate attribute"));
                }
                let slot = match key.as_str() {
                    "strength" => &mut result.strength,
                    "speed" => &mut result.speed,
                    "intellect" => &mut result.intellect,
                    "willpower" => &mut result.willpower,
                    "awareness" => &mut result.awareness,
                    "presence" => &mut result.presence,
                    _ => {
                        return Err(serde::de::Error::custom(format!(
                            "unknown attribute {key:?}"
                        )));
                    }
                };
                *slot = map.next_value()?;
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Visitor(std::marker::PhantomData))
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub priority: i16,
    pub kind: Option<String>,
    #[serde(default)]
    pub add_subtypes: Vec<String>,
    #[serde(default)]
    pub remove_subtypes: Vec<String>,
    #[serde(default, deserialize_with = "partial_attributes")]
    pub adjustments: ByAttribute<i16>,
    #[serde(default, deserialize_with = "partial_attributes")]
    pub overrides: ByAttribute<Option<u16>>,
    #[serde(default)]
    pub modifiers: Vec<Modifier>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advancement {
    pub source: HitDieSource,
    #[serde(default)]
    pub training: Vec<Skill>,
    pub talent: Option<Talent>,
    pub attribute: Option<Attribute>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSpec {
    pub species: String,
    pub name: String,
    pub faction: String,
    pub attributes: Option<ByAttribute<u16>>,
    pub binding: ManaBinding,
    pub hit_dice: Vec<Advancement>,
    #[serde(default)]
    pub templates: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(default)]
    pub species: BTreeMap<String, Species>,
    #[serde(default)]
    pub templates: BTreeMap<String, Template>,
}
fn subtypes(values: &[String]) -> Result<BTreeSet<SubtypeId>, Failure> {
    let compiled = values
        .iter()
        .map(|id| SubtypeId::new(id).map_err(|_| invalid(format!("Invalid subtype {id:?}"))))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if compiled.len() != values.len() {
        return Err(invalid("Duplicate creature subtype"));
    }
    Ok(compiled)
}
#[derive(Clone, Debug)]
pub struct CompiledCatalog {
    content: Arc<s::creatures::Content>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum CatalogKind {
    Species,
    Template,
}

impl Catalog {
    pub fn is_empty(&self) -> bool {
        self.species.is_empty() && self.templates.is_empty()
    }
    pub fn compile(&self, rules: Arc<s::rules::Ruleset>) -> Result<CompiledCatalog, Failure> {
        self.compile_with_context(rules, |_, _, failure| failure)
    }

    pub(crate) fn compile_with_context(
        &self,
        rules: Arc<s::rules::Ruleset>,
        context: impl Fn(CatalogKind, &str, Failure) -> Failure,
    ) -> Result<CompiledCatalog, Failure> {
        if self.species.len() > s::limits::CATALOG_ENTRIES
            || self.templates.len() > s::limits::CATALOG_ENTRIES
        {
            return Err(invalid("Creature catalog exceeds definition limits"));
        }
        let mut catalog = s::creatures::ScenarioCatalog::default();
        for (id, author) in &self.species {
            let (compiled_id, compiled) = (|| -> Result<_, Failure> {
                let id = SpeciesId::new(id)
                    .map_err(|_| invalid(format!("Invalid species ID {id:?}")))?;
                let compiled = s::creatures::Species {
                    id: id.clone(),
                    kind: TypeId::new(&author.kind)
                        .map_err(|_| invalid("Invalid creature type ID"))?,
                    subtypes: subtypes(&author.subtypes)?,
                    default_attributes: s::attributes::Attributes::new(author.attributes)
                        .map_err(|_| invalid("Invalid initial attributes"))?,
                    anatomy: author
                        .anatomy
                        .as_ref()
                        .map(|anatomy| anatomy.compile(&rules))
                        .transpose()?
                        .unwrap_or_else(|| s::AnatomySpec::from_rules(&rules)),
                    melee: author.melee.compile(&rules)?,
                    modifiers: author
                        .modifiers
                        .iter()
                        .map(Modifier::compile)
                        .collect::<Result<_, _>>()?,
                };
                Ok((id, compiled))
            })()
            .map_err(|failure| context(CatalogKind::Species, id, failure))?;
            catalog.species.insert(compiled_id, compiled);
        }
        for (id, author) in &self.templates {
            let (compiled_id, compiled) = (|| -> Result<_, Failure> {
                let id = TemplateId::new(id)
                    .map_err(|_| invalid(format!("Invalid template ID {id:?}")))?;
                let compiled = s::creatures::Template {
                    id: id.clone(),
                    priority: author.priority,
                    kind: author
                        .kind
                        .as_ref()
                        .map(|id| TypeId::new(id).map_err(|_| invalid("Invalid creature type ID")))
                        .transpose()?,
                    add_subtypes: subtypes(&author.add_subtypes)?,
                    remove_subtypes: subtypes(&author.remove_subtypes)?,
                    adjustments: author.adjustments,
                    overrides: author.overrides,
                    modifiers: author
                        .modifiers
                        .iter()
                        .map(Modifier::compile)
                        .collect::<Result<_, _>>()?,
                };
                Ok((id, compiled))
            })()
            .map_err(|failure| context(CatalogKind::Template, id, failure))?;
            catalog.templates.insert(compiled_id, compiled);
        }
        // Diagnose a rejected definition by its typed owner, without interpreting
        // error strings. The extra isolated checks run only after validation fails.
        if let Err(error) = catalog.validate(&rules) {
            for (id, species) in &catalog.species {
                let candidate = s::creatures::ScenarioCatalog {
                    species: BTreeMap::from([(id.clone(), species.clone())]),
                    ..Default::default()
                };
                if let Err(error) = candidate.validate(&rules) {
                    return Err(context(
                        CatalogKind::Species,
                        id.as_str(),
                        invalid(format!("Invalid creature catalog: {error:?}")),
                    ));
                }
            }
            for (id, template) in &catalog.templates {
                let candidate = s::creatures::ScenarioCatalog {
                    templates: BTreeMap::from([(id.clone(), template.clone())]),
                    ..Default::default()
                };
                if let Err(error) = candidate.validate(&rules) {
                    return Err(context(
                        CatalogKind::Template,
                        id.as_str(),
                        invalid(format!("Invalid creature catalog: {error:?}")),
                    ));
                }
            }
            return Err(invalid(format!("Invalid creature catalog: {error:?}")));
        }
        let content = s::creatures::Content::new(rules, catalog)
            .map_err(|error| invalid(format!("Invalid creature catalog: {error:?}")))?;
        Ok(CompiledCatalog { content })
    }
}
#[derive(Clone, Debug)]
pub struct Recipe {
    prototype: s::creatures::CreatureBuild,
    identity: s::CreatureIdentity,
}
impl CompiledCatalog {
    pub fn content(&self) -> &Arc<s::creatures::Content> {
        &self.content
    }
    pub(crate) fn template(&self, id: &str) -> Option<&s::creatures::Template> {
        self.content
            .catalog()
            .templates
            .get(&TemplateId::new(id).ok()?)
    }
    pub fn prepare(&self, author: &BuildSpec) -> Result<Recipe, Failure> {
        if author.hit_dice.is_empty()
            || author.hit_dice.len() > s::limits::HIT_DICE
            || author.templates.len() > s::limits::TEMPLATES
        {
            return Err(invalid(
                "Authored creatures require 1 to 256 hit dice and at most 32 templates",
            ));
        }
        for (label, value, maximum) in [
            ("name", &author.name, s::CreatureIdentity::MAX_NAME_BYTES),
            (
                "faction",
                &author.faction,
                s::CreatureIdentity::MAX_FACTION_BYTES,
            ),
        ] {
            if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
                return Err(invalid(format!(
                    "Creature {label} must be a nonempty label of at most {maximum} bytes without control characters"
                )));
            }
        }
        let id = SpeciesId::new(&author.species).map_err(|_| invalid("Invalid species ID"))?;
        let species = self
            .content
            .catalog()
            .species
            .get(&id)
            .ok_or_else(|| invalid(format!("Unknown creature species {:?}", author.species)))?;
        let attributes = author
            .attributes
            .map(s::attributes::Attributes::new)
            .transpose()
            .map_err(|_| invalid("Invalid initial attributes"))?
            .unwrap_or(species.default_attributes);
        let templates = author
            .templates
            .iter()
            .map(|id| TemplateId::new(id).map_err(|_| invalid("Invalid template ID")))
            .collect::<Result<Vec<_>, _>>()?;
        let sources = author
            .hit_dice
            .iter()
            .map(|entry| hit_die_source(&entry.source, self.content.rules()))
            .collect::<Result<Vec<_>, _>>()?;
        let ledger = s::progression::HdLedger::seeded(sources, 0)
            .map_err(|_| invalid("Invalid creature hit-dice ledger"))?;
        let choices = author
            .hit_dice
            .iter()
            .map(|entry| {
                Ok(s::creatures::HdChoices {
                    training: entry
                        .training
                        .iter()
                        .map(|value| skill(value))
                        .collect::<Result<_, _>>()?,
                    talent: entry
                        .talent
                        .as_ref()
                        .map(|value| talent(value, self.content.rules()))
                        .transpose()?,
                    attribute: entry
                        .attribute
                        .as_ref()
                        .map(|value| attribute(value))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        let prototype = s::creatures::CreatureBuild::from_recorded(
            self.content.clone(),
            id,
            attributes,
            ledger,
            primitive(&author.binding, "mana binding")?,
            choices,
            templates,
        )
        .map_err(|error| invalid(format!("Invalid authored creature build: {error:?}")))?;
        let derived = prototype
            .derive()
            .map_err(|error| invalid(format!("Invalid authored creature build: {error:?}")))?;
        if !derived.dormant_talents.is_empty() {
            return Err(invalid(
                "Initial talent choices must be eligible in the authored build",
            ));
        }
        Ok(Recipe {
            prototype,
            identity: s::CreatureIdentity {
                name: author.name.clone(),
                faction: author.faction.clone(),
            },
        })
    }
}
impl Recipe {
    pub fn identity(&self) -> &s::CreatureIdentity {
        &self.identity
    }
    pub fn instantiate(&self, seed: u64) -> Result<s::creatures::CreatureBuild, Failure> {
        let build = &self.prototype;
        let ledger = s::progression::HdLedger::seeded(
            build
                .ledger()
                .entries()
                .iter()
                .map(|entry| entry.source())
                .collect(),
            seed,
        )
        .map_err(|_| invalid("Invalid creature hit-dice ledger"))?;
        s::creatures::CreatureBuild::from_recorded(
            build.content().clone(),
            build.species_id().clone(),
            build.initial_attributes(),
            ledger,
            build.binding(),
            build.choices().to_vec(),
            build.template_ids().to_vec(),
        )
        .map_err(|error| invalid(format!("Invalid creature spawn: {error:?}")))
    }
}

/// Stable, domain-separated spawn stream. Actor allocation order, region
/// loading and combat draws cannot change an already assigned actor's HD seeds.
pub fn actor_health_seed(game_seed: u64, actor: u64) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"tor-creature-health-v1\0");
    hash.update(game_seed.to_le_bytes());
    hash.update(actor.to_le_bytes());
    let bytes = hash.finalize();
    u64::from_le_bytes(bytes[..8].try_into().expect("eight digest bytes"))
}

#[cfg(test)]
mod strict_stat_tests {
    use super::*;

    #[test]
    fn creature_stat_forms_preserve_authoring_shapes_and_compile() {
        for stat in [
            Stat::Health,
            Stat::Edge,
            Stat::CheckEdge,
            Stat::IncomingEdge { defense: None },
            Stat::IncomingEdge {
                defense: Some("physical".into()),
            },
            Stat::Attribute {
                attribute: "strength".into(),
            },
            Stat::Defense {
                defense: "cognitive".into(),
            },
            Stat::ResourceMax {
                resource: "stamina".into(),
            },
            Stat::Ability {
                ability: "fear".into(),
                stat: "check_edge".into(),
            },
            Stat::AbilityTag {
                tag: "weapon_attack".into(),
                stat: "check_bonus".into(),
            },
        ] {
            let json = serde_json::to_value(&stat).unwrap();
            assert_eq!(serde_json::from_value::<Stat>(json.clone()).unwrap(), stat);
            let source = toml::to_string(&stat).unwrap();
            assert_eq!(toml::from_str::<Stat>(&source).unwrap(), stat);
            assert_eq!(
                serde_json::to_value(serde_json::from_value::<Stat>(json.clone()).unwrap())
                    .unwrap(),
                json
            );
            Modifier {
                scope: Scope::Always,
                op: Op::Add { stat, value: 1 },
            }
            .compile()
            .unwrap();
        }
    }

    #[test]
    fn creature_stat_authoring_rejects_unknown_fields_in_json_and_toml() {
        let mut accepted = Vec::new();
        for kind in ["health", "edge", "check_edge"] {
            let json = serde_json::json!({"kind": kind, "private_owner": 999});
            if serde_json::from_value::<Stat>(json).is_ok() {
                accepted.push((kind, "json"));
            }
            let toml = format!("kind = '{kind}'\nprivate_owner = 999\n");
            if toml::from_str::<Stat>(&toml).is_ok() {
                accepted.push((kind, "toml"));
            }
        }
        assert!(
            accepted.is_empty(),
            "unknown fields accepted for {accepted:?}"
        );
    }
}
