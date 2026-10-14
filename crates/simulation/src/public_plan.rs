//! Captured public intent and observer-owned, one-time plan observations.
//!
//! These are backend facts. Wire adapters project only the already-disclosed
//! historical result, never current actor identity or location.
use crate::content_ids::AbilityId;
use crate::{ActorId, Game};
use serde::{Deserialize, Serialize};
use tor_world::{Location, Position, RegionId, SightCell};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedAbility {
    pub id: AbilityId,
    pub name: String,
}
impl<'de> Deserialize<'de> for ObservedAbility {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: AbilityId,
            #[serde(deserialize_with = "read_name")]
            name: String,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            id: fields.id,
            name: fields.name,
        })
    }
}

/// Observer-relative coordinates and opaque-cell input frozen at observation.
/// The backend location is never a wire coordinate or selectable actor handle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ObservedAim {
    Unavailable,
    Disclosed {
        location: Location,
        occurrences: Vec<Position>,
    },
}
impl<'de> Deserialize<'de> for ObservedAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unavailable {},
            Disclosed {
                #[serde(deserialize_with = "read_location")]
                location: Location,
                #[serde(deserialize_with = "read_occurrences")]
                occurrences: Vec<Position>,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Unavailable {} => Self::Unavailable,
            Fields::Disclosed {
                location,
                occurrences,
            } => Self::Disclosed {
                location,
                occurrences,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicPlanState {
    /// Uniform for idle, recovery and private/non-ability work. Public recovery
    /// needs separate licensed provenance; private ready_at is not that model.
    NoPublicPlan,
    Preparing {
        ability: Option<ObservedAbility>,
        aim: ObservedAim,
        remaining_work: u64,
    },
}
impl<'de> Deserialize<'de> for PublicPlanState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            NoPublicPlan {},
            Preparing {
                #[serde(deserialize_with = "crate::bounded::required_option")]
                ability: Option<ObservedAbility>,
                aim: ObservedAim,
                remaining_work: u64,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::NoPublicPlan {} => Self::NoPublicPlan,
            Fields::Preparing {
                ability,
                aim,
                remaining_work,
            } => Self::Preparing {
                ability,
                aim,
                remaining_work,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PublicPlanObservation {
    pub observed_at: u64,
    pub subject_name: String,
    pub state: PublicPlanState,
}
impl<'de> Deserialize<'de> for PublicPlanObservation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            observed_at: u64,
            #[serde(deserialize_with = "read_name")]
            subject_name: String,
            state: PublicPlanState,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            observed_at: fields.observed_at,
            subject_name: fields.subject_name,
            state: fields.state,
        })
    }
}
impl PublicPlanObservation {
    pub(crate) fn valid(&self, rules: &crate::rules::Ruleset, world: &tor_world::World) -> bool {
        valid_name(&self.subject_name)
            && match &self.state {
                PublicPlanState::NoPublicPlan => true,
                PublicPlanState::Preparing {
                    ability,
                    aim,
                    remaining_work,
                } => {
                    *remaining_work <= crate::limits::NUMERIC as u64
                        && ability.as_ref().is_none_or(|known| {
                            rules
                                .ability(&known.id)
                                .is_some_and(|definition| definition.name == known.name)
                        })
                        && match aim {
                            ObservedAim::Unavailable => true,
                            ObservedAim::Disclosed {
                                location,
                                occurrences,
                            } => {
                                world.knows(*location)
                                    && !occurrences.is_empty()
                                    && occurrences.len() <= crate::limits::PUBLIC_AIM_OCCURRENCES
                                    && occurrences.windows(2).all(|pair| pair[0] < pair[1])
                            }
                        }
                }
            }
    }
}
fn valid_name(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= crate::limits::DISPLAY_NAME_BYTES
        && !value.chars().any(char::is_control)
}
fn read_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct Name;
    impl serde::de::Visitor<'_> for Name {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded display name")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if valid_name(value) {
                Ok(value.to_owned())
            } else {
                Err(E::custom("invalid display name"))
            }
        }
    }
    deserializer.deserialize_str(Name)
}
fn read_occurrences<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Position>, D::Error> {
    struct NamedPosition(Position);
    impl<'de> Deserialize<'de> for NamedPosition {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            read_position(deserializer).map(Self)
        }
    }
    crate::bounded::Bounded::<NamedPosition, { crate::limits::PUBLIC_AIM_OCCURRENCES }>::deserialize(deserializer)
        .map(|rows| rows.0.into_iter().map(|row| row.0).collect())
}

/// The declared aim captured with preparation, independently of a moving target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicAim {
    Unavailable,
    Cell { location: Location },
}

impl<'de> Deserialize<'de> for PublicAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unavailable {},
            Cell {
                #[serde(deserialize_with = "read_location")]
                location: Location,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Unavailable {} => Self::Unavailable,
            Fields::Cell { location } => Self::Cell { location },
        })
    }
}

fn read_position<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Position, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        x: i32,
        y: i32,
        z: i32,
    }
    let fields: Fields = crate::bounded::named_map(deserializer)?;
    Ok(Position {
        x: fields.x,
        y: fields.y,
        z: fields.z,
    })
}

fn read_location<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Location, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        region: RegionId,
        #[serde(deserialize_with = "read_position")]
        position: Position,
    }
    let fields: Fields = crate::bounded::named_map(deserializer)?;
    Ok(Location {
        region: fields.region,
        position: fields.position,
    })
}

/// Opaque, bounded work prefilter for one committed observation boundary.
/// These backend identities grant no disclosure authority. Only an ordinary
/// observer view can license additions to remembered ability knowledge.
pub struct ObservationKnowledgeCandidates(std::collections::BTreeSet<AbilityId>);
impl ObservationKnowledgeCandidates {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Game {
    /// Scan the event log once, rather than once for every potential observer.
    /// Valid games contain at most the bounded ability catalog's distinct IDs.
    pub fn observation_knowledge_candidates(
        &self,
    ) -> Result<ObservationKnowledgeCandidates, crate::GameError> {
        let mut ids = std::collections::BTreeSet::new();
        for event in &self.event_log {
            if let crate::combat::CombatEvent::Resolved { outcome, .. } = event {
                if let Some(id) = &outcome.ability {
                    ids.insert(id.clone());
                    if ids.len() > crate::limits::CATALOG_ENTRIES {
                        return Err(crate::GameError::Rule(
                            crate::abilities::RuleError::InvalidPlan,
                        ));
                    }
                }
            }
        }
        Ok(ObservationKnowledgeCandidates(ids))
    }

    /// Conservative only: an unknown backend event may require a final view,
    /// but is never itself evidence that this observer saw the ability.
    pub fn may_add_observation_knowledge(
        &self,
        observer: ActorId,
        candidates: &ObservationKnowledgeCandidates,
    ) -> bool {
        self.actors.get(&observer).is_some_and(|actor| {
            candidates
                .0
                .iter()
                .any(|id| !actor.recognized_abilities.contains(id))
        })
    }

    /// Persist only ability identities already licensed by this committed
    /// observer view. The host calls this at the authoritative perception
    /// boundary, never in response to a read or connection-cache fill.
    pub fn record_observation_knowledge(
        &mut self,
        observation: &crate::Observation,
    ) -> Result<(), crate::GameError> {
        let known = &self
            .actors
            .get(&observation.actor)
            .ok_or(crate::GameError::UnknownActor)?
            .recognized_abilities;
        let mut learned = std::collections::BTreeSet::new();
        for event in observation
            .combat
            .as_ref()
            .into_iter()
            .flat_map(|combat| combat.events.iter())
        {
            if let crate::combat::DisclosedCombatEvent::Ability { id, name, .. } = event {
                if self
                    .content
                    .rules
                    .ability(id)
                    .is_none_or(|definition| definition.name != *name)
                {
                    return Err(crate::GameError::Rule(
                        crate::abilities::RuleError::InvalidPlan,
                    ));
                }
                if !known.contains(id) {
                    learned.insert(id.clone());
                }
            }
        }
        if !learned.is_empty() {
            self.actors
                .get_mut(&observation.actor)
                .ok_or(crate::GameError::UnknownActor)?
                .recognized_abilities
                .extend(learned);
        }
        Ok(())
    }

    /// A past paid result, never a fresh inspection or selectable target handle.
    pub fn public_plan_observation(&self, observer: ActorId) -> Option<&PublicPlanObservation> {
        self.actors.get(&observer)?.public_plan.as_deref()
    }

    pub(crate) fn capture_public_plan_observation(
        &self,
        observer: ActorId,
        subject: ActorId,
    ) -> Option<PublicPlanObservation> {
        let actor = self.actors.get(&subject)?;
        let owner = self.actors.get(&observer)?;
        if observer == subject || !actor.alive() || !owner.alive() {
            return None;
        }
        let scene = self.scene(observer).ok()?;
        let cells = scene.iter().map(|cell| cell.location).collect();
        if !self
            .actors
            .perceived(&self.world, &cells)
            .iter()
            .any(|(id, _)| *id == subject)
        {
            return None;
        }
        // Classification grants disclosure; a timing helper alone never does.
        let state = if let Some(plan) = actor
            .pending
            .as_ref()
            .and_then(|pending| pending.plan.as_ability())
        {
            let ability = owner
                .recognized_abilities
                .contains(&plan.ability)
                .then(|| self.content.rules.ability(&plan.ability))
                .flatten()
                .map(|definition| ObservedAbility {
                    id: definition.id.clone(),
                    name: definition.name.clone(),
                });
            let aim = match plan.public_aim {
                PublicAim::Cell { location } => {
                    let occurrences: Vec<_> = scene
                        .iter()
                        .filter(|cell| cell.location == location)
                        .map(|cell| cell.offset)
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    if occurrences.is_empty() {
                        ObservedAim::Unavailable
                    } else {
                        ObservedAim::Disclosed {
                            location,
                            occurrences,
                        }
                    }
                }
                _ => ObservedAim::Unavailable,
            };
            PublicPlanState::Preparing {
                ability,
                aim,
                remaining_work: self.preparation_timing(subject)?.preparation_remaining?,
            }
        } else {
            PublicPlanState::NoPublicPlan
        };
        let name = crate::observation::projected_actor_name(owner, actor);
        Some(PublicPlanObservation {
            observed_at: self.tick,
            subject_name: if name.is_empty() {
                "figure".into()
            } else {
                name.to_owned()
            },
            state,
        })
    }

    /// Selection can see only part of a body. Never substitute a hidden anchor
    /// or an arbitrary contact for the explicitly captured public target cell.
    pub(crate) fn capture_public_aim(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: Option<&[SightCell]>,
    ) -> PublicAim {
        let Some(target) = self.actors.get(&target) else {
            return PublicAim::Unavailable;
        };
        let visible = sight.map_or_else(
            || self.scene_contains(actor, target.location).unwrap_or(false),
            |scene| scene.iter().any(|cell| cell.location == target.location),
        );
        if visible {
            PublicAim::Cell {
                location: target.location,
            }
        } else {
            PublicAim::Unavailable
        }
    }
}

pub(crate) fn read_recognized_abilities<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<tor_world::Shared<std::collections::BTreeSet<AbilityId>>, D::Error> {
    let values =
        crate::bounded::Bounded::<AbilityId, { crate::limits::CATALOG_ENTRIES }>::deserialize(
            deserializer,
        )?
        .0;
    let count = values.len();
    let known: std::collections::BTreeSet<_> = values.into_iter().collect();
    if known.len() != count {
        return Err(serde::de::Error::custom("duplicate recognized ability"));
    }
    Ok(tor_world::Shared::new(known))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observer() -> (Game, ActorId) {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut game = Game::two_room_in_stone(42, build.content().clone());
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "observer".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        (game, actor)
    }

    #[test]
    fn repeated_observation_without_new_knowledge_preserves_shared_storage_and_state() {
        let (mut game, actor) = observer();
        let ability = game
            .actors
            .get(&actor)
            .unwrap()
            .recognized_abilities
            .iter()
            .next()
            .unwrap()
            .clone();
        let name = game.content.rules.ability(&ability).unwrap().name.clone();
        let mut view = game.observe(actor).unwrap();
        for repeat_known_event in [false, true] {
            view.combat.as_mut().unwrap().events = if repeat_known_event {
                vec![
                    crate::combat::DisclosedCombatEvent::Ability {
                        caster: Some(actor),
                        target: Some(actor),
                        id: ability.clone(),
                        name: name.clone(),
                        outcome: crate::abilities::OutcomeStatus::Applied,
                        strike: None,
                    };
                    2
                ]
            } else {
                vec![]
            };
            let before = game.clone();
            let known = game
                .actors
                .get(&actor)
                .unwrap()
                .recognized_abilities
                .clone();
            let counts = crate::diagnostics::work_counts();
            game.record_observation_knowledge(&view).unwrap();
            assert_eq!(game, before);
            assert!(game
                .actors
                .get(&actor)
                .unwrap()
                .recognized_abilities
                .shares_storage(&known));
            assert_eq!(
                crate::diagnostics::work_counts().actor_table_copies,
                counts.actor_table_copies
            );
        }
    }

    #[test]
    fn ordinary_damage_without_new_grants_preserves_recognition_storage() {
        let (mut game, actor) = observer();
        let known = game
            .actors
            .get(&actor)
            .unwrap()
            .recognized_abilities
            .clone();
        let before = game.creature(actor).unwrap().health().injury();
        crate::test_creatures::injure(&mut game, actor, 1).unwrap();
        assert_eq!(game.creature(actor).unwrap().health().injury(), before + 1);
        assert_eq!(
            &*game.actors.get(&actor).unwrap().recognized_abilities,
            &*known
        );
        assert!(game
            .actors
            .get(&actor)
            .unwrap()
            .recognized_abilities
            .shares_storage(&known));
    }
    #[test]
    fn public_plan_idle_and_private_work_have_one_uniform_frozen_result() {
        use crate::abilities::{PlannedEffect, ResolutionPlan, Snapshot};
        use crate::{Action, EquipmentSlotId, EquipmentSpec, IntentionOrigin, ItemClass, ItemSpec};
        let (mut idle, observer) = observer();
        let subject = idle
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(300).unwrap(),
            )
            .unwrap();
        idle.configure_creature(
            subject,
            crate::CreatureIdentity {
                name: "visible subject".into(),
                faction: "neutral".into(),
            },
            crate::test_creatures::build(crate::test_creatures::species()),
        )
        .unwrap();
        let slot = idle
            .actors
            .get(&subject)
            .unwrap()
            .anatomy
            .slots
            .iter()
            .position(|slot| slot.as_str() == "body_armor")
            .unwrap();
        let mut equipment = ItemSpec::ordinary("private mail".into());
        equipment.class = ItemClass::Armor;
        equipment.equipment = Some(EquipmentSpec {
            slot: crate::EquipmentSlot::new("body_armor").unwrap(),
            attack: None,
            modifiers: vec![],
        });
        let item = idle
            .place_item_stack(
                10,
                idle.actors.get(&subject).unwrap().location,
                Some(subject),
                1,
                equipment,
            )
            .unwrap();
        let mut private = idle.clone();
        for game in [&mut idle, &mut private] {
            game.act(observer, Action::Wait).unwrap();
        }
        idle.act(subject, Action::Wait).unwrap();
        private
            .admit_intention(
                subject,
                Action::Equip {
                    item,
                    slot: EquipmentSlotId(slot as u16),
                },
                IntentionOrigin::Human,
            )
            .unwrap();
        private.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(idle.tick(), private.tick());
        assert_eq!(
            idle.observe(observer).unwrap(),
            private.observe(observer).unwrap(),
            "public evidence must match before observing"
        );
        assert!(idle.preparation(subject).is_none());
        assert!(private
            .preparation(subject)
            .unwrap()
            .plan
            .as_ability()
            .is_none());
        let expected = idle
            .capture_public_plan_observation(observer, subject)
            .unwrap();
        let actual = private
            .capture_public_plan_observation(observer, subject)
            .unwrap();
        assert_eq!(
            actual, expected,
            "private work must not create a separate status, label, aim or exact-work payload"
        );
        assert_eq!(
            serde_json::to_value(&actual).unwrap()["state"],
            serde_json::json!({"kind":"no_public_plan"})
        );
        let effects = [
            PlannedEffect::ObservePublicPlan,
            PlannedEffect::ApplyQualifiedConditionToSource {
                id: crate::content_ids::ConditionId::new("readied").unwrap(),
                duration: 300,
                ability_tag: crate::content_ids::AbilityTagId::new("weapon_attack").unwrap(),
            },
        ];
        for game in [&mut idle, &mut private] {
            let input = ResolutionPlan::Effects {
                actor: observer,
                target: subject,
                effects: &effects,
            };
            let target = game.creature(subject).unwrap().into_owned();
            let snapshot = Snapshot::capture(&target)
                .with_context(game.resolution_context(input, observer, subject).unwrap());
            let result = crate::abilities::resolve(input, &snapshot, &mut 42, &mut ());
            assert_eq!(result.effects.len(), 2);
            game.apply(result).unwrap();
            assert_eq!(game.public_plan_observation(observer), Some(&expected));
        }
        assert_eq!(
            idle.creature(observer).unwrap(),
            private.creature(observer).unwrap(),
            "Readied grant and recipient timing are identical"
        );
        let before = private.clone();
        assert_eq!(private.public_plan_observation(observer), Some(&expected));
        assert_eq!(
            private, before,
            "inspection does not advance or reclassify private work"
        );
        private.pause_preparation(subject).unwrap();
        assert_eq!(private.public_plan_observation(observer), Some(&expected));
        let mut shared = crate::checkpoint::SharedState::default();
        let checkpoint = private.checkpoint(&mut shared);
        let restored =
            Game::restore_checkpoint(checkpoint, &shared, private.content().clone()).unwrap();
        assert_eq!(restored.public_plan_observation(observer), Some(&expected));
    }
    fn restored(game: &Game) -> Result<Game, crate::checkpoint::PersistError> {
        let mut shared = crate::checkpoint::SharedState::default();
        let checkpoint = game.checkpoint(&mut shared);
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone())
    }
    #[test]
    fn public_plan_restore_requires_knowledge_of_current_own_grants() {
        let (game, actor) = observer();
        assert!(restored(&game).is_ok(), "untampered current actor is valid");
        let mut missing = game.clone();
        missing
            .actors
            .get_mut(&actor)
            .unwrap()
            .recognized_abilities
            .clear();
        assert!(
            restored(&missing).is_err(),
            "a current own grant cannot become undiscovered on restore"
        );
    }
    #[test]
    fn public_plan_restore_rejects_a_historical_name_without_observer_license() {
        let (mut game, actor) = observer();
        let id = AbilityId::new("magic_bolt").unwrap();
        assert!(!game
            .actors
            .get(&actor)
            .unwrap()
            .recognized_abilities
            .contains(&id));
        let ability = game.content.rules.ability(&id).unwrap();
        let record = PublicPlanObservation {
            observed_at: game.tick(),
            subject_name: "remembered foe".into(),
            state: PublicPlanState::Preparing {
                ability: Some(ObservedAbility {
                    id: id.clone(),
                    name: ability.name.clone(),
                }),
                aim: ObservedAim::Unavailable,
                remaining_work: 100,
            },
        };
        game.actors.get_mut(&actor).unwrap().public_plan = Some(tor_world::Shared::new(record));
        assert!(
            restored(&game).is_err(),
            "valid catalog identity is not an observer license"
        );
        game.actors
            .get_mut(&actor)
            .unwrap()
            .recognized_abilities
            .insert(id);
        assert!(
            restored(&game).is_ok(),
            "licensed historical names remain valid"
        );
    }
    #[test]
    fn public_plan_restore_never_reveals_a_historical_unknown_after_recognition() {
        let (mut game, actor) = observer();
        let record = PublicPlanObservation {
            observed_at: game.tick(),
            subject_name: "remembered foe".into(),
            state: PublicPlanState::Preparing {
                ability: None,
                aim: ObservedAim::Unavailable,
                remaining_work: 100,
            },
        };
        game.actors.get_mut(&actor).unwrap().public_plan =
            Some(tor_world::Shared::new(record.clone()));
        game.actors
            .get_mut(&actor)
            .unwrap()
            .recognized_abilities
            .insert(AbilityId::new("magic_bolt").unwrap());
        assert_eq!(
            restored(&game).unwrap().public_plan_observation(actor),
            Some(&record)
        );
        game.actors
            .get_mut(&actor)
            .unwrap()
            .public_plan
            .as_mut()
            .unwrap()
            .observed_at = game.tick() + 1;
        assert!(
            restored(&game).is_err(),
            "future records are not historical knowledge"
        );
    }
    fn retained_record(game: &Game, actor: ActorId) -> PublicPlanObservation {
        PublicPlanObservation {
            observed_at: game.tick(),
            subject_name: "remembered foe".into(),
            state: PublicPlanState::Preparing {
                ability: None,
                remaining_work: 200,
                aim: ObservedAim::Disclosed {
                    location: game.actors.get(&actor).unwrap().location,
                    occurrences: (0..1024).map(|x| Position { x, y: 0, z: 0 }).collect(),
                },
            },
        }
    }
    fn aim_storage(record: &PublicPlanObservation) -> *const Position {
        let PublicPlanState::Preparing {
            aim: ObservedAim::Disclosed { occurrences, .. },
            ..
        } = &record.state
        else {
            panic!("disclosed historical record")
        };
        occurrences.as_ptr()
    }
    fn retain_record(game: &mut Game, actor: ActorId, record: PublicPlanObservation) {
        game.apply(crate::abilities::Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: crate::abilities::OutcomeStatus::Applied,
            check: None,
            participation: crate::abilities::AttemptedParticipation::source(),
            target_rider_participated: false,
            effects: vec![crate::abilities::ResolvedEffect::ObservePublicPlan {
                observation: tor_world::Shared::new(record),
            }],
        })
        .unwrap();
    }
    #[test]
    fn public_plan_storage_ordinary_lifecycle_keeps_the_same_immutable_offsets() {
        let (mut game, actor) = observer();
        let record = retained_record(&game, actor);
        retain_record(&mut game, actor, record);
        let old = game.clone();
        let pointer = aim_storage(old.public_plan_observation(actor).unwrap());
        crate::test_creatures::injure(&mut game, actor, 1).unwrap();
        assert_eq!(
            game.public_plan_observation(actor),
            old.public_plan_observation(actor)
        );
        assert_eq!(
            aim_storage(game.public_plan_observation(actor).unwrap()),
            pointer,
            "ordinary lifecycle snapshots must not clone an unchanged historical payload"
        );
    }
    #[test]
    fn public_plan_storage_application_context_and_outcome_clones_share_offsets() {
        use crate::abilities::{
            Outcome, OutcomeStatus, PlannedEffect, ResolutionPlan, ResolvedEffect,
        };
        let (mut game, actor) = observer();
        let record = retained_record(&game, actor);
        retain_record(&mut game, actor, record);
        let stored = game
            .actors
            .get(&actor)
            .unwrap()
            .public_plan
            .as_ref()
            .unwrap();
        let pointer = aim_storage(stored);
        let effects = [PlannedEffect::ApplyQualifiedConditionToSource {
            id: crate::content_ids::ConditionId::new("readied").unwrap(),
            duration: 300,
            ability_tag: crate::content_ids::AbilityTagId::new("weapon_attack").unwrap(),
        }];
        let context = game
            .resolution_context(
                ResolutionPlan::Effects {
                    actor,
                    target: actor,
                    effects: &effects,
                },
                actor,
                actor,
            )
            .unwrap();
        assert_eq!(
            aim_storage(
                context
                    .source
                    .as_ref()
                    .unwrap()
                    .observed_plan
                    .as_ref()
                    .unwrap()
            ),
            pointer
        );
        assert_eq!(
            aim_storage(
                context
                    .clone()
                    .source
                    .as_ref()
                    .unwrap()
                    .observed_plan
                    .as_ref()
                    .unwrap()
            ),
            pointer
        );
        let outcome = Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            participation: crate::abilities::AttemptedParticipation::source(),
            target_rider_participated: false,
            effects: vec![ResolvedEffect::ObservePublicPlan {
                observation: stored.clone(),
            }],
        };
        let retained = outcome.clone();
        let ResolvedEffect::ObservePublicPlan { observation } = &retained.effects[0] else {
            unreachable!()
        };
        assert_eq!(
            aim_storage(observation),
            pointer,
            "retained outcome/history snapshots share the immutable payload too"
        );
    }
    #[test]
    fn public_plan_storage_replacement_preserves_the_prior_snapshot_and_save_shape() {
        let (mut game, actor) = observer();
        let record = retained_record(&game, actor);
        let canonical = serde_json::to_value(&record).unwrap();
        retain_record(&mut game, actor, record.clone());
        let old = game.clone();
        let mut replacement = record.clone();
        replacement.subject_name = "a later selected foe".into();
        retain_record(&mut game, actor, replacement.clone());
        assert_eq!(old.public_plan_observation(actor), Some(&record));
        assert_eq!(game.public_plan_observation(actor), Some(&replacement));
        let captured = crate::actor_record::ActorRecord::capture(old.actors.get(&actor).unwrap());
        assert_eq!(
            serde_json::to_value(captured).unwrap()["public_plan"],
            canonical,
            "ownership must not introduce any saved wrapper or schema change"
        );
    }
    // Append inside public_plan::tests after the frozen batch.
    #[test]
    fn public_plan_name_uses_the_same_unnamed_fallback_as_ordinary_projection() {
        let (mut baseline, observer) = observer();
        let subject = baseline
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 2, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let build = crate::test_creatures::build(crate::test_creatures::species());
        for name in ["   ", "  Guard  ", "Guard"] {
            let mut game = baseline.clone();
            game.configure_creature(
                subject,
                crate::CreatureIdentity {
                    name: name.into(),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
            let view = game.observe(observer).unwrap();
            let ordinary = &view
                .visible_actors
                .iter()
                .find(|actor| actor.id == subject)
                .unwrap()
                .name;
            let public = game
                .capture_public_plan_observation(observer, subject)
                .unwrap();
            if name.trim().is_empty() {
                assert_eq!(ordinary, "", "ordinary unnamed path stays generic");
                assert_eq!(public.subject_name, "figure");
            } else {
                assert_eq!(ordinary, name, "valid authored name is preserved exactly");
                assert_eq!(public.subject_name, *ordinary);
            }
            assert!(public.valid(game.content().rules(), &game.world));
        }
    }
    #[test]
    fn public_plan_recipient_invalid_later_source_effect_rolls_back_the_whole_payload() {
        use crate::abilities::{Outcome, OutcomeStatus, ResolvedEffect, RuleError};
        let (mut game, actor) = observer();
        let before = game.clone();
        let mut invalid = retained_record(&game, actor);
        invalid.observed_at = game.tick() + 1;
        let result = game.apply(Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            participation: crate::abilities::AttemptedParticipation::source(),
            target_rider_participated: false,
            effects: vec![
                ResolvedEffect::ApplyQualifiedConditionToSource {
                    id: crate::content_ids::ConditionId::new("readied").unwrap(),
                    duration: 300,
                    qualification: crate::conditions::TokenQualification {
                        opponent: actor,
                        ability_tag: crate::content_ids::AbilityTagId::new("weapon_attack")
                            .unwrap(),
                    },
                },
                ResolvedEffect::ObservePublicPlan {
                    observation: tor_world::Shared::new(invalid),
                },
            ],
        });
        assert_eq!(result, Err(crate::GameError::Rule(RuleError::InvalidPlan)));
        assert_eq!(
            game, before,
            "no token, record, lifecycle event or actor state from a rejected payload may escape"
        );
    }

    fn replace_retained_aim(record: &mut PublicPlanObservation, location: Location) {
        let PublicPlanState::Preparing {
            aim: ObservedAim::Disclosed { location: at, .. },
            ..
        } = &mut record.state
        else {
            panic!("disclosed aim fixture")
        };
        *at = location;
    }
    fn malformed_historical_aim(location: Location) {
        let (mut game, actor) = observer();
        let mut record = retained_record(&game, actor);
        assert!(restored(&game).is_ok(), "untampered control");
        replace_retained_aim(&mut record, location);
        game.actors.get_mut(&actor).unwrap().public_plan = Some(tor_world::Shared::new(record));
        assert!(
            restored(&game).is_err(),
            "impossible historical address {location:?} must not restore"
        );
    }
    #[test]
    fn public_plan_review_restore_rejects_zero_region_aim() {
        malformed_historical_aim(Location {
            region: RegionId(0),
            position: Position { x: 1, y: 1, z: 0 },
        });
    }
    #[test]
    fn public_plan_review_restore_rejects_missing_region_aim() {
        malformed_historical_aim(Location {
            region: RegionId(999),
            position: Position { x: 1, y: 1, z: 0 },
        });
    }
    #[test]
    fn public_plan_review_restore_rejects_out_of_bounds_aim() {
        malformed_historical_aim(Location {
            region: RegionId(1),
            position: Position {
                x: i32::MAX,
                y: 1,
                z: 0,
            },
        });
    }
    #[test]
    fn public_plan_review_apply_rejects_impossible_aim_without_partial_publication() {
        use crate::abilities::{Outcome, OutcomeStatus, ResolvedEffect, RuleError};
        let (mut game, actor) = observer();
        let mut record = retained_record(&game, actor);
        replace_retained_aim(
            &mut record,
            Location {
                region: RegionId(999),
                position: Position { x: 1, y: 1, z: 0 },
            },
        );
        let before = game.clone();
        let result = game.apply(Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: OutcomeStatus::Applied,
            check: None,
            participation: crate::abilities::AttemptedParticipation::source(),
            target_rider_participated: false,
            effects: vec![ResolvedEffect::ObservePublicPlan {
                observation: tor_world::Shared::new(record),
            }],
        });
        assert_eq!(result, Err(crate::GameError::Rule(RuleError::InvalidPlan)));
        assert_eq!(game, before);
    }
    #[test]
    fn public_plan_review_changed_wall_preserves_the_historical_aim() {
        let (mut game, actor) = observer();
        let aim = Location {
            region: RegionId(1),
            position: Position { x: 3, y: 1, z: 0 },
        };
        let mut record = retained_record(&game, actor);
        replace_retained_aim(&mut record, aim);
        retain_record(&mut game, actor, record.clone());
        game.set_wall(aim, true).unwrap();
        let after = restored(&game).expect("wall changes do not invalidate past observations");
        assert_eq!(after.public_plan_observation(actor), Some(&record));
    }
    #[test]
    fn public_plan_review_detached_region_preserves_aim_without_loading_or_pinning() {
        use crate::streaming::{MemoryRecords, RegionState, RegionTransition};
        let (mut game, actor) = observer();
        let region = RegionId(3);
        game.add_chamber(tor_world::Region {
            id: region,
            name: "historical gallery".into(),
            bounds: tor_world::Extent::new(3, 3, 2).unwrap(),
        })
        .unwrap();
        let aim = Location {
            region,
            position: Position { x: 1, y: 1, z: 0 },
        };
        let mut record = retained_record(&game, actor);
        replace_retained_aim(&mut record, aim);
        retain_record(&mut game, actor, record.clone());
        let proposed = RegionTransition {
            active: std::collections::BTreeSet::from([RegionId(1), RegionId(2)]),
            loaded: std::collections::BTreeSet::from([RegionId(1), RegionId(2)]),
        };
        let mut records = MemoryRecords::default();
        let (_, report) = game.transition_regions(&proposed, &mut records).unwrap();
        assert_eq!(report.detached, vec![region]);
        assert_eq!(game.region_state(region), Some(RegionState::Detached));
        let after =
            restored(&game).expect("historical addresses remain valid in detached metadata");
        assert_eq!(after.region_state(region), Some(RegionState::Detached));
        assert!(after.world.region(region).is_none());
        assert!(after.world.knows(aim));
        assert_eq!(after.public_plan_observation(actor), Some(&record));
    }
    #[test]
    fn public_plan_review_retained_outcome_rejects_invalid_historical_aim() {
        let (mut game, actor) = observer();
        let record = retained_record(&game, actor);
        retain_record(&mut game, actor, record.clone());
        let events = game.event_log.clone();
        assert!(events
            .iter()
            .any(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. })));
        game.act(actor, crate::Action::Wait).unwrap();
        assert!(game.tick() > record.observed_at);
        game.event_log = events;
        assert!(
            restored(&game).is_ok(),
            "older retained outcomes remain historical"
        );
        for event in &mut game.event_log {
            let crate::combat::CombatEvent::Resolved { outcome, .. } = event else {
                continue;
            };
            for effect in &mut outcome.effects {
                if let crate::abilities::ResolvedEffect::ObservePublicPlan { observation } = effect
                {
                    replace_retained_aim(
                        observation,
                        Location {
                            region: RegionId(999),
                            position: Position { x: 1, y: 1, z: 0 },
                        },
                    );
                }
            }
        }
        assert_eq!(
            game.public_plan_observation(actor),
            Some(&record),
            "actor copy remains valid to isolate retained outcome validation"
        );
        assert!(
            restored(&game).is_err(),
            "a saved outcome must not smuggle an impossible historical address"
        );
    }

    #[test]
    fn public_plan_review_publication_and_saved_outcome_reject_inconsistent_evidence() {
        use crate::abilities::{
            AttemptedParticipation, CheckParticipation, Outcome, OutcomeStatus,
            PayloadParticipation, ResolvedEffect, UnselectedReason,
        };
        let (game, actor) = observer();
        let valid = Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: OutcomeStatus::Unaffected,
            check: None,
            participation: AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![ResolvedEffect::Heal { amount: 0 }],
        };
        let mut control = game.clone();
        control.apply(valid.clone()).unwrap();
        assert!(restored(&control).is_ok());
        for participation in [
            AttemptedParticipation::source(),
            AttemptedParticipation::empty(),
            AttemptedParticipation {
                check: CheckParticipation::TargetInvolved,
                payload: PayloadParticipation::Target,
            },
            AttemptedParticipation {
                check: CheckParticipation::None,
                payload: PayloadParticipation::Unselected {
                    reason: UnselectedReason::Dead,
                },
            },
        ] {
            let mut invalid = valid.clone();
            invalid.participation = participation;
            let mut candidate = game.clone();
            assert!(candidate.apply(invalid.clone()).is_err());
            assert_eq!(candidate, game, "invalid evidence cannot publish any state");
            candidate
                .event_log
                .push(crate::combat::CombatEvent::Resolved {
                    outcome: invalid,
                    strike: None,
                });
            assert!(
                restored(&candidate).is_err(),
                "typed in-memory checkpoint cannot bypass the byte reader"
            );
        }
    }

    #[test]
    fn public_plan_status_review_typed_restore_rejects_noncanonical_results() {
        use crate::abilities::{
            AttemptedParticipation, CheckParticipation, Outcome, OutcomeStatus,
            PayloadParticipation, ResolvedCheck, ResolvedEffect,
        };
        let (game, actor) = observer();
        let base = Outcome {
            actor,
            target: actor,
            ability: None,
            intention: None,
            result: OutcomeStatus::Unaffected,
            check: None,
            participation: AttemptedParticipation::empty(),
            target_rider_participated: false,
            effects: vec![],
        };
        // Trusted callers retain the established result-hint normalization.
        for (hint, effects, expected) in [
            (OutcomeStatus::Applied, vec![], OutcomeStatus::Unaffected),
            (
                OutcomeStatus::Unaffected,
                vec![ResolvedEffect::Damage { amount: 1 }],
                OutcomeStatus::Applied,
            ),
        ] {
            let mut candidate = game.clone();
            let mut input = base.clone();
            input.result = hint;
            input.effects = effects;
            input.participation = AttemptedParticipation::target();
            candidate.apply(input).unwrap();
            assert!(candidate.combat_events().iter().any(|event| matches!(event, crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.result == expected)));
            assert!(
                restored(&candidate).is_ok(),
                "publication has already normalized its retained result"
            );
        }
        let mut invalid = Vec::new();
        let mut empty_applied = base.clone();
        empty_applied.result = OutcomeStatus::Applied;
        invalid.push(("empty Applied", empty_applied));
        let mut damage_unaffected = base.clone();
        damage_unaffected.participation = AttemptedParticipation::target();
        damage_unaffected.effects = vec![ResolvedEffect::Damage { amount: 1 }];
        invalid.push(("positive damage Unaffected", damage_unaffected));
        let mut failed_empty = base.clone();
        failed_empty.check = Some(ResolvedCheck {
            actor: crate::rules::CheckActor::User,
            success: false,
            opposed: false,
        });
        failed_empty.participation.check = CheckParticipation::UserOnly;
        invalid.push(("failed User empty Unaffected", failed_empty.clone()));
        for (label, payload) in [
            ("source Miss", PayloadParticipation::Source),
            ("target Miss", PayloadParticipation::Target),
        ] {
            let mut miss = failed_empty.clone();
            miss.result = OutcomeStatus::Miss;
            miss.participation.payload = payload;
            invalid.push((label, miss));
        }
        let accepted: Vec<_> = invalid
            .into_iter()
            .filter_map(|(label, outcome)| {
                let mut candidate = game.clone();
                candidate
                    .event_log
                    .push(crate::combat::CombatEvent::Resolved {
                        outcome,
                        strike: None,
                    });
                restored(&candidate).is_ok().then_some(label)
            })
            .collect();
        assert!(
            accepted.is_empty(),
            "typed restore accepted noncanonical outcome status: {accepted:?}"
        );
    }

    #[test]
    fn public_plan_status_review_coherent_erased_history_is_not_authenticated() {
        use crate::abilities::{AttemptedParticipation, Outcome, OutcomeStatus};
        let (game, actor) = observer();
        // With all effects erased, both are internally coherent historical facts.
        // Checkpoint restoration validates shape and consistency, not original intent.
        for participation in [
            AttemptedParticipation::source(),
            AttemptedParticipation::target(),
        ] {
            let mut candidate = game.clone();
            candidate
                .event_log
                .push(crate::combat::CombatEvent::Resolved {
                    outcome: Outcome {
                        actor,
                        target: actor,
                        ability: Some(AbilityId::new("basic_melee").unwrap()),
                        intention: None,
                        result: OutcomeStatus::Unaffected,
                        check: None,
                        participation,
                        target_rider_participated: false,
                        effects: vec![],
                    },
                    strike: Some(0),
                });
            assert!(
                restored(&candidate).is_ok(),
                "coherent user-edited history is outside the existing authenticity guarantee"
            );
        }
    }

    #[test]
    fn public_plan_rider_review_restored_summary_requires_primary_event_context() {
        use crate::abilities::{AttemptedParticipation, Outcome, OutcomeStatus};
        let (game, actor) = observer();
        let outcome = Outcome {
            actor,
            target: actor,
            ability: Some(AbilityId::new("basic_melee").unwrap()),
            intention: None,
            result: OutcomeStatus::Unaffected,
            check: None,
            participation: AttemptedParticipation::source(),
            target_rider_participated: true,
            effects: vec![],
        };
        for strike in [None, Some(crate::limits::STRIKES as u16)] {
            let mut candidate = game.clone();
            candidate
                .event_log
                .push(crate::combat::CombatEvent::Resolved {
                    outcome: outcome.clone(),
                    strike,
                });
            assert!(
                restored(&candidate).is_err(),
                "a rider summary requires a valid primary strike context"
            );
        }
    }
}
