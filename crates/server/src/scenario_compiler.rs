//! Immutable manifest definitions prepared for deterministic region construction.
//! Region instances remain lazy; this context contains no mutable game state and
//! is reconstructed rather than serialized into a save.
use super::{fail, label, require, Actor, Archetype, CombatSpec, Failure, Item, Manifest};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
    pub anatomy: Option<Cow<'a, tor_simulation::AnatomySpec>>,
    pub known_identities: Vec<String>,
    pub combat: Option<Cow<'a, tor_simulation::combat::CombatSpec>>,
    pub body: Option<Cow<'a, tor_simulation::BodySpec>>,
    pub control: PreparedControl,
    pub velocity: Option<[i64; 3]>,
    pub asset: Option<Cow<'a, str>>,
}

impl PreparedCreature<'_> {
    pub fn borrowed(&self) -> PreparedCreature<'_> {
        PreparedCreature {
            anatomy: self.anatomy.as_deref().map(Cow::Borrowed),
            known_identities: self.known_identities.clone(),
            combat: self.combat.as_deref().map(Cow::Borrowed),
            body: self.body.as_deref().map(Cow::Borrowed),
            control: self.control.clone(),
            velocity: self.velocity,
            asset: self.asset.as_deref().map(Cow::Borrowed),
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct PreparedArchetype {
    pub anatomy: Option<tor_simulation::AnatomySpec>,
    pub equipment: Option<tor_simulation::EquipmentSpec>,
    pub consumable: Option<tor_simulation::ConsumableSpec>,
    pub class: tor_simulation::ItemClass,
    pub combat: Option<tor_simulation::combat::CombatSpec>,
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
    fn new(author: &Archetype, manifest: &Manifest) -> Self {
        Self {
            anatomy: author.anatomy.clone().map(Into::into),
            equipment: author.equipment.clone().map(Into::into),
            consumable: author.consumable.clone().map(Into::into),
            class: author.class.into(),
            combat: author.combat.clone().map(Into::into),
            body: author.body.clone().map(Into::into),
            identity: author.identity.clone(),
            concealed: author.appearance_pool.is_some(),
            stackable: author.stackable,
            properties: author.properties.clone(),
            name: author.name.clone(),
            turn_ticks: author.turn_ticks,
            actor_asset: author.asset.clone(),
            item_asset: item_asset(author, manifest).map(str::to_owned),
        }
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
    archetypes: BTreeMap<String, PreparedArchetype>,
    ordinary: PreparedArchetype,
    pub characters: Vec<PreparedCharacter>,
    ai_profiles: BTreeMap<String, Arc<tor_simulation::ai::AiProfile>>,
    omitted: BTreeSet<u64>,
    pub objective_item: Option<u64>,
}

impl PreparedDefinitions {
    pub fn new(manifest: &Manifest, selected: u64) -> Self {
        let ai_profiles: BTreeMap<_, _> = manifest
            .ai_profiles
            .iter()
            .map(|(name, author)| (name.clone(), Arc::new(author.clone().into())))
            .collect();
        let characters: Vec<_> = manifest
            .characters
            .iter()
            .map(|author| PreparedCharacter {
                id: author.id,
                anchor: author.anchor.clone(),
                turn_ticks: author.turn_ticks,
                creature: PreparedCreature {
                    anatomy: author
                        .anatomy
                        .clone()
                        .map(|anatomy| Cow::Owned(anatomy.into())),
                    known_identities: author.known_identities.clone(),
                    combat: author
                        .combat
                        .clone()
                        .or_else(|| manifest.objective.as_ref().map(|_| CombatSpec::default()))
                        .map(|spec| Cow::Owned(spec.into())),
                    body: author.body.clone().map(|body| Cow::Owned(body.into())),
                    control: if author.id == selected {
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
            .collect();
        let omitted = characters
            .iter()
            .filter(|c| !c.creature.control.spawned())
            .map(|c| c.id)
            .collect();
        Self {
            archetypes: manifest
                .archetypes
                .iter()
                .map(|(name, author)| (name.clone(), PreparedArchetype::new(author, manifest)))
                .collect(),
            ordinary: PreparedArchetype::default(),
            characters,
            ai_profiles,
            omitted,
            objective_item: manifest.objective.as_ref().and_then(|o| o.item),
        }
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

    pub fn actor(&self, author: &Actor) -> Result<PreparedCreature<'_>, Failure> {
        let inherited = self.archetype(&author.archetype).map_err(Failure::from)?;
        Ok(PreparedCreature {
            anatomy: author
                .anatomy
                .clone()
                .map(|anatomy| Cow::Owned(anatomy.into()))
                .or_else(|| inherited.anatomy.as_ref().map(Cow::Borrowed)),
            known_identities: author.known_identities.clone(),
            combat: author
                .combat
                .clone()
                .map(|spec| Cow::Owned(spec.into()))
                .or_else(|| inherited.combat.as_ref().map(Cow::Borrowed)),
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
    use super::*;

    #[test]
    fn concealed_item_cannot_override_its_physical_class() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/items");
        let package = super::super::read_package(&root).unwrap();
        let prepared = PreparedDefinitions::new(&package.manifest, 1);
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
            PreparedArchetype::new(arrow, &manifest)
                .item_asset
                .as_deref(),
            Some("item.arrow")
        );
        let hidden = &manifest.archetypes["healing"];
        assert_eq!(item_asset(hidden, &manifest), Some("item.shared"));
        assert_eq!(
            PreparedArchetype::new(hidden, &manifest)
                .item_asset
                .as_deref(),
            Some("item.shared")
        );
        manifest.appearance_pools.get_mut("potions").unwrap().asset = None;
        let hidden = &manifest.archetypes["healing"];
        assert_eq!(item_asset(hidden, &manifest), None);
        assert_eq!(PreparedArchetype::new(hidden, &manifest).item_asset, None);
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
        let prepared = PreparedDefinitions::new(&manifest, 1);
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
        let selected_other = PreparedDefinitions::new(&manifest, 2);
        assert!(selected_other.characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .is_none());
        assert!(selected_other.omitted_carrier(Some(1)));
        manifest.characters[1].ai = Some("missing".into());
        let deferred = PreparedDefinitions::new(&manifest, 1);
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
        assert!(PreparedDefinitions::new(&manifest, 2).characters[1]
            .creature
            .control
            .profile()
            .unwrap()
            .is_none());
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
        manifest.archetypes.insert(
            "guard".into(),
            Archetype {
                anatomy: None,
                equipment: None,
                consumable: None,
                class: Default::default(),
                combat: Some(CombatSpec {
                    name: "guard".into(),
                    max_hp: 41,
                    ..Default::default()
                }),
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
        let prepared = PreparedDefinitions::new(&manifest, 1);
        let guard = prepared.archetype(&Some("guard".into())).unwrap();
        assert_eq!(guard.combat.as_ref().unwrap().name, "guard");
        assert_eq!(guard.combat.as_ref().unwrap().max_hp, 41);
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
        assert!(ordinary.combat.is_none() && ordinary.body.is_none() && !ordinary.stackable);
        assert!(prepared.archetype(&Some("missing".into())).is_err());
    }

    #[test]
    fn character_combat_is_implicit_only_when_an_objective_exists() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
        let mut manifest = super::super::read_package(&root).unwrap().manifest;
        assert!(manifest.objective.is_none());
        assert!(PreparedDefinitions::new(&manifest, 1).characters[0]
            .creature
            .combat
            .is_none());
        manifest.objective = Some(super::super::Objective {
            anchor: "1/start".into(),
            item: Some(7),
            disclosed: true,
            continue_play: false,
        });
        let prepared = PreparedDefinitions::new(&manifest, 1);
        assert_eq!(
            prepared.characters[0].creature.combat.as_deref(),
            Some(&tor_simulation::combat::CombatSpec::default())
        );
        assert_eq!(prepared.objective_item, Some(7));
        manifest.characters[0].combat = Some(CombatSpec {
            name: "explicit".into(),
            max_hp: 41,
            ..Default::default()
        });
        assert_eq!(
            PreparedDefinitions::new(&manifest, 1).characters[0]
                .creature
                .combat
                .as_ref()
                .unwrap()
                .name,
            "explicit"
        );
    }
}
