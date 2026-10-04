//! Immutable manifest definitions prepared for deterministic region construction.
//! Region instances remain lazy; this context contains no mutable game state and
//! is reconstructed rather than serialized into a save.
use super::{fail, Archetype, CombatSpec, Failure, Manifest};
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(super) struct PreparedArchetype {
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
            combat: author.combat.clone().map(Into::into),
            body: author.body.clone().map(Into::into),
            identity: author.identity.clone(),
            concealed: author.appearance_pool.is_some(),
            stackable: author.stackable,
            properties: author.properties.clone(),
            name: author.name.clone(),
            turn_ticks: author.turn_ticks,
            actor_asset: author.asset.clone(),
            item_asset: match &author.appearance_pool {
                Some(pool) => manifest
                    .appearance_pools
                    .get(pool)
                    .and_then(|p| p.asset.clone()),
                None => author.asset.clone(),
            },
        }
    }
}

#[derive(Debug)]
pub(super) struct PreparedCharacter {
    pub id: u64,
    pub selected: bool,
    pub unselected: String,
    pub combat: Option<tor_simulation::combat::CombatSpec>,
    pub body: Option<tor_simulation::BodySpec>,
    pub ai: Option<String>,
    pub velocity: Option<[i64; 3]>,
    pub asset: Option<String>,
    pub known_identities: Vec<String>,
}

#[derive(Debug)]
pub(super) struct PreparedDefinitions {
    archetypes: BTreeMap<String, PreparedArchetype>,
    ordinary: PreparedArchetype,
    pub characters: Vec<PreparedCharacter>,
    pub ai_profiles: BTreeMap<String, tor_simulation::ai::AiProfile>,
    pub objective_item: Option<u64>,
}

impl PreparedDefinitions {
    pub fn new(manifest: &Manifest, selected: u64) -> Self {
        Self {
            archetypes: manifest
                .archetypes
                .iter()
                .map(|(name, author)| (name.clone(), PreparedArchetype::new(author, manifest)))
                .collect(),
            ordinary: PreparedArchetype::default(),
            characters: manifest
                .characters
                .iter()
                .map(|author| PreparedCharacter {
                    id: author.id,
                    selected: author.id == selected,
                    unselected: author.unselected.clone(),
                    combat: author
                        .combat
                        .clone()
                        .or_else(|| manifest.objective.as_ref().map(|_| CombatSpec::default()))
                        .map(Into::into),
                    body: author.body.clone().map(Into::into),
                    ai: author.ai.clone(),
                    velocity: author.velocity,
                    asset: author.asset.clone(),
                    known_identities: author.known_identities.clone(),
                })
                .collect(),
            ai_profiles: manifest
                .ai_profiles
                .iter()
                .map(|(name, author)| (name.clone(), author.clone().into()))
                .collect(),
            objective_item: manifest.objective.as_ref().and_then(|o| o.item),
        }
    }

    pub fn archetype(&self, key: &Option<String>) -> Result<&PreparedArchetype, Failure> {
        match key {
            Some(key) => self
                .archetypes
                .get(key)
                .ok_or_else(|| fail(format!("Unknown archetype {key}"))),
            None => Ok(&self.ordinary),
        }
    }

    pub fn omitted_carrier(&self, carrier: Option<u64>) -> bool {
        carrier.is_some_and(|id| {
            self.characters
                .iter()
                .any(|c| c.id == id && !c.selected && c.unselected == "omit")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario_package::{AiProfile, AppearancePool, BodySpec};
    use std::path::Path;

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
            prepared.characters[0].combat,
            Some(tor_simulation::combat::CombatSpec::default())
        );
        assert_eq!(prepared.objective_item, Some(7));
        manifest.characters[0].combat = Some(CombatSpec {
            name: "explicit".into(),
            max_hp: 41,
            ..Default::default()
        });
        assert_eq!(
            PreparedDefinitions::new(&manifest, 1).characters[0]
                .combat
                .as_ref()
                .unwrap()
                .name,
            "explicit"
        );
    }
}
