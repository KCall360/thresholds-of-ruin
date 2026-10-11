//! Author-owned creature declarations and immutable spawn recipes. Runtime
//! records, wire DTOs and authored catalogs have separate schemas. Compilation
//! delegates rules and ownership validation to the shared creature model.
use crate::{
    scenario_package::{AnatomySpec, DamageType},
    Failure,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use tor_simulation as s;

fn invalid(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

macro_rules! catalog {
    ($name:ident, $backend:path; $($variant:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }
        impl From<$name> for $backend {
            fn from(value: $name) -> Self { match value { $($name::$variant => Self::$variant),+ } }
        }
    };
}
catalog!(CreatureType, s::progression::CreatureType;
    Aberration, Animal, Construct, Dragon, Elemental, Fey, Giant, Humanoid,
    MagicalBeast, MonstrousHumanoid, Ooze, Outsider, Plant, Undead, Vermin);
catalog!(Subtype, s::creatures::Subtype;
    Air, Angel, Aquatic, Archon, Augmented, Chaotic, Cold, Earth, Evil,
    Extraplanar, Fire, Goblinoid, Good, Incorporeal, Lawful, Native, Reptilian,
    Shapechanger, Swarm, Water);
catalog!(Skill, s::attributes::Skill;
    Athletics, HeavyWeaponry, Agility, LightWeaponry, Stealth, Thievery,
    Crafting, Deduction, Lore, Medicine, Discipline, Intimidation, Insight,
    Perception, Survival, Deception, Leadership, Persuasion, Spellcasting);
catalog!(Attribute, s::attributes::Attribute; Strength, Speed, Intellect, Willpower, Awareness, Presence);
catalog!(ManaBinding, s::attributes::ManaBinding; Intellect, Willpower, Awareness, Presence);
catalog!(Ability, s::grants::Ability; BasicMelee, PowerStrike, MagicBolt, Fear);
catalog!(Descriptor, s::grants::Descriptor; Fire, Cold, Fear, MindAffecting);
catalog!(Talent, s::talents::Talent;
    Hardiness, Toughness, Unyielding, Indomitable, Endurance, DeepEndurance,
    Tireless, Guard, GreaterGuard, IronGuard, HeavyBlows, MightyBlows,
    CrushingBlows, PerfectedBlows, ImpactWard, KeenWard, EnergyWard, Resolve,
    PowerStrike, MagicBolt, Fear, ArcaneReserve, PotentBolt, EmpoweredBolt,
    GreaterBolt, MasterBolt, FearMastery);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitDieSource {
    Racial,
    Warrior,
    Mage,
}
impl From<HitDieSource> for s::progression::HdSource {
    fn from(value: HitDieSource) -> Self {
        use s::progression::{Class, HdSource};
        match value {
            HitDieSource::Racial => HdSource::Racial,
            HitDieSource::Warrior => HdSource::Class(Class::Warrior),
            HitDieSource::Mage => HdSource::Class(Class::Mage),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attributes {
    pub strength: u8,
    pub speed: u8,
    pub intellect: u8,
    pub willpower: u8,
    pub awareness: u8,
    pub presence: u8,
}
impl Attributes {
    fn compile(self) -> Result<s::attributes::Attributes, Failure> {
        s::attributes::Attributes::new([
            self.strength,
            self.speed,
            self.intellect,
            self.willpower,
            self.awareness,
            self.presence,
        ])
        .map_err(|_| invalid("Ordinary initial attributes must be between zero and five"))
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Adjustments {
    pub strength: i16,
    pub speed: i16,
    pub intellect: i16,
    pub willpower: i16,
    pub awareness: i16,
    pub presence: i16,
}
impl Adjustments {
    fn values(self) -> [i16; 6] {
        [
            self.strength,
            self.speed,
            self.intellect,
            self.willpower,
            self.awareness,
            self.presence,
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Overrides {
    pub strength: Option<u16>,
    pub speed: Option<u16>,
    pub intellect: Option<u16>,
    pub willpower: Option<u16>,
    pub awareness: Option<u16>,
    pub presence: Option<u16>,
}
impl Overrides {
    fn values(self) -> BTreeMap<s::attributes::Attribute, u16> {
        s::attributes::Attribute::ALL
            .into_iter()
            .zip([
                self.strength,
                self.speed,
                self.intellect,
                self.willpower,
                self.awareness,
                self.presence,
            ])
            .filter_map(|(attribute, value)| value.map(|value| (attribute, value)))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selector {
    Category { category: DamageType },
    Descriptor { descriptor: Descriptor },
}
impl From<Selector> for s::grants::Selector {
    fn from(value: Selector) -> Self {
        match value {
            Selector::Category { category } => Self::Category(category.into()),
            Selector::Descriptor { descriptor } => Self::Descriptor(descriptor.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Grant {
    Health { amount: u32 },
    Stamina { amount: u32 },
    Focus { amount: u32 },
    Mana { amount: u32 },
    PhysicalDefense { amount: i32 },
    MeleeFlat { amount: i32 },
    MeleeDice { amount: u16 },
    BoltFlat { amount: i32 },
    BoltDice { amount: u16 },
    FearDifficulty { amount: i32 },
    FearDuration { amount: u64 },
    Immunity { selector: Selector },
    Reduction { selector: Selector, amount: u32 },
    Ability { ability: Ability },
    Mindless,
    Magical,
}
impl From<Grant> for s::grants::Grant {
    fn from(value: Grant) -> Self {
        match value {
            Grant::Health { amount } => Self::Health(amount),
            Grant::Stamina { amount } => Self::Stamina(amount),
            Grant::Focus { amount } => Self::Focus(amount),
            Grant::Mana { amount } => Self::Mana(amount),
            Grant::PhysicalDefense { amount } => Self::PhysicalDefense(amount),
            Grant::MeleeFlat { amount } => Self::MeleeFlat(amount),
            Grant::MeleeDice { amount } => Self::MeleeDice(amount),
            Grant::BoltFlat { amount } => Self::BoltFlat(amount),
            Grant::BoltDice { amount } => Self::BoltDice(amount),
            Grant::FearDifficulty { amount } => Self::FearDifficulty(amount),
            Grant::FearDuration { amount } => Self::FearDuration(amount),
            Grant::Immunity { selector } => Self::Immunity(selector.into()),
            Grant::Reduction { selector, amount } => Self::Reduction(selector.into(), amount),
            Grant::Ability { ability } => Self::Ability(ability.into()),
            Grant::Mindless => Self::Mindless,
            Grant::Magical => Self::Magical,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Species {
    pub kind: CreatureType,
    #[serde(default)]
    pub subtypes: Vec<Subtype>,
    pub attributes: Attributes,
    pub anatomy: Option<AnatomySpec>,
    pub melee: s::attacks::MeleeAttackRecord,
    #[serde(default)]
    pub grants: Vec<Grant>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub priority: i16,
    pub kind: Option<CreatureType>,
    #[serde(default)]
    pub add_subtypes: Vec<Subtype>,
    #[serde(default)]
    pub remove_subtypes: Vec<Subtype>,
    #[serde(default)]
    pub adjustments: Adjustments,
    #[serde(default)]
    pub overrides: Overrides,
    #[serde(default)]
    pub grants: Vec<Grant>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advancement {
    pub source: HitDieSource,
    #[serde(default)]
    pub training: Vec<Skill>,
    pub talent: Option<Talent>,
    pub attribute: Option<Attribute>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildSpec {
    pub species: String,
    pub name: String,
    pub faction: String,
    pub attributes: Option<Attributes>,
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

fn subtypes(values: &[Subtype]) -> Result<BTreeSet<s::creatures::Subtype>, Failure> {
    let compiled: BTreeSet<_> = values.iter().copied().map(Into::into).collect();
    if compiled.len() != values.len() {
        return Err(invalid("Duplicate creature subtype"));
    }
    Ok(compiled)
}

#[derive(Clone, Debug)]
pub struct CompiledCatalog {
    species: BTreeMap<String, s::creatures::Species>,
    templates: BTreeMap<String, s::creatures::Template>,
}

impl Catalog {
    pub fn is_empty(&self) -> bool {
        self.species.is_empty() && self.templates.is_empty()
    }

    pub fn compile(&self) -> Result<CompiledCatalog, Failure> {
        if self.species.len() > 4_096 || self.templates.len() > 4_096 {
            return Err(invalid(
                "Creature catalogs are limited to 4096 species and 4096 templates",
            ));
        }
        let mut species = BTreeMap::new();
        for (id, author) in &self.species {
            let compiled = s::creatures::Species {
                id: id.clone(),
                kind: author.kind.into(),
                subtypes: subtypes(&author.subtypes)?,
                default_attributes: author.attributes.compile()?,
                anatomy: author
                    .anatomy
                    .clone()
                    .map(Into::into)
                    .unwrap_or_else(s::AnatomySpec::humanoid),
                melee: author
                    .melee
                    .restore()
                    .map_err(|_| invalid(format!("Species {id:?}: invalid melee definition")))?,
                grants: author.grants.iter().copied().map(Into::into).collect(),
            };
            prototype(compiled.clone())
                .map_err(|error| invalid(format!("Species {id:?}: {error:?}")))?;
            species.insert(id.clone(), compiled);
        }
        let mut templates = BTreeMap::new();
        for (id, author) in &self.templates {
            let compiled = s::creatures::Template {
                id: id.clone(),
                priority: author.priority,
                kind: author.kind.map(Into::into),
                add_subtypes: subtypes(&author.add_subtypes)?,
                remove_subtypes: subtypes(&author.remove_subtypes)?,
                adjustments: author.adjustments.values(),
                overrides: author.overrides.values(),
                grants: author.grants.iter().copied().map(Into::into).collect(),
            };
            let probe = s::creatures::Species {
                id: "template_validation".into(),
                kind: s::progression::CreatureType::Humanoid,
                subtypes: BTreeSet::new(),
                default_attributes: s::attributes::Attributes::default(),
                anatomy: s::AnatomySpec::humanoid(),
                melee: tor_simulation::attacks::MeleeAttack::new(
                    s::attributes::Skill::HeavyWeaponry,
                    0,
                    60,
                    40,
                    {
                        let component = tor_simulation::damage::DamageComponent::rolled(
                            tor_simulation::combat::DamageType::Impact,
                            None,
                            s::dice::DicePool::new(1, 6, 0).expect("fixed valid dice"),
                        );
                        let primary = component.key();
                        tor_simulation::damage::DamageSpec::new(vec![component], Some(primary))
                            .unwrap()
                    },
                )
                .unwrap(),
                grants: vec![],
            };
            let mut build = prototype(probe)
                .map_err(|error| invalid(format!("Template validation: {error:?}")))?;
            build
                .set_templates(vec![compiled.clone()])
                .map_err(|error| invalid(format!("Template {id:?}: {error:?}")))?;
            templates.insert(id.clone(), compiled);
        }
        Ok(CompiledCatalog { species, templates })
    }
}

fn prototype(
    species: s::creatures::Species,
) -> Result<s::creatures::CreatureBuild, s::creatures::BuildError> {
    s::creatures::CreatureBuild::new(
        species,
        s::progression::HdLedger::seeded(vec![s::progression::HdSource::Racial], 0)
            .expect("one valid hit die"),
        s::attributes::ManaBinding::Intellect,
    )
}

#[derive(Clone, Debug)]
pub struct Recipe {
    prototype: s::creatures::CreatureBuild,
    identity: s::CreatureIdentity,
}

impl CompiledCatalog {
    pub(crate) fn template(&self, id: &str) -> Option<&s::creatures::Template> {
        self.templates.get(id)
    }

    pub fn prepare(&self, author: &BuildSpec) -> Result<Recipe, Failure> {
        if author.hit_dice.is_empty() || author.hit_dice.len() > 256 || author.templates.len() > 32
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
        let species = self
            .species
            .get(&author.species)
            .ok_or_else(|| invalid(format!("Unknown creature species {:?}", author.species)))?
            .clone();
        let attributes = author
            .attributes
            .map(Attributes::compile)
            .transpose()?
            .unwrap_or(species.default_attributes);
        let templates = author
            .templates
            .iter()
            .map(|id| {
                self.templates
                    .get(id)
                    .cloned()
                    .ok_or_else(|| invalid(format!("Unknown creature template {id:?}")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ledger = s::progression::HdLedger::seeded(
            author
                .hit_dice
                .iter()
                .map(|entry| entry.source.into())
                .collect(),
            0,
        )
        .map_err(|_| invalid("Invalid creature hit-dice ledger"))?;
        let choices = author
            .hit_dice
            .iter()
            .map(|entry| s::creatures::HdChoices {
                training: entry.training.iter().copied().map(Into::into).collect(),
                talent: entry.talent.map(Into::into),
                attribute: entry.attribute.map(Into::into),
            })
            .collect();
        let prototype = s::creatures::CreatureBuild::from_recorded(
            species,
            attributes,
            ledger,
            author.binding.into(),
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

    /// The recipe contains no mutable actor state. Spawn seeds affect only the
    /// independent per-HD health records, never combat randomness or choices.
    pub fn instantiate(&self, seed: u64) -> Result<s::creatures::CreatureBuild, Failure> {
        let build = &self.prototype;
        let sources = build
            .ledger()
            .entries()
            .iter()
            .map(|entry| entry.source())
            .collect();
        let ledger = s::progression::HdLedger::seeded(sources, seed)
            .map_err(|_| invalid("Invalid creature hit-dice ledger"))?;
        s::creatures::CreatureBuild::from_recorded(
            build.species().clone(),
            build.initial_attributes(),
            ledger,
            build.binding(),
            build.choices().to_vec(),
            build.templates().to_vec(),
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
mod identity_limit_tests {
    #[test]
    fn creature_identity_limits_reject_names_before_instantiation() {
        let manifest: crate::scenario_package::Manifest = toml::from_str(include_str!(
            "../../../scenarios/tests/interactions/scenario.toml"
        ))
        .unwrap();
        let catalog = manifest.creatures.compile().unwrap();
        let mut author = manifest.characters[0].creature.clone().unwrap();
        author.name = "x".repeat(60);
        assert!(catalog.prepare(&author).is_ok());
        author.name.push('x');
        let error = catalog
            .prepare(&author)
            .expect_err("runtime cannot accept 61-byte names");
        assert!(error.message.contains("Creature name"));
        assert!(error.message.contains("60 bytes"));
    }
}
