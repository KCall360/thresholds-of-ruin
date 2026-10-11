//! Creature-owned actor lifecycle and strict checkpoint adapter.
use crate::combat::{AttackSpec, CombatSpec, DamageType};
use crate::creatures::{CreatureBuild, CreatureState, RebuildOutcome};
use crate::grants::Selector;
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Shared;

/// Trusted backend inspection of one loaded creature. This is deliberately not
/// serializable: callers authorize recipients and map independent wire DTOs.
/// Borrowed definitions, choices and derived sources all belong to this state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureInspection<'a> {
    pub identity: CreatureIdentity,
    pub creature: &'a CreatureState,
    pub stats: crate::creatures::OwnStats,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatureIdentity {
    #[serde(deserialize_with = "read_name")]
    pub name: String,
    #[serde(deserialize_with = "read_faction")]
    pub faction: String,
}

fn read_text<'de, D: serde::Deserializer<'de>, const MAXIMUM: usize>(
    deserializer: D,
) -> Result<String, D::Error> {
    struct Text<const MAXIMUM: usize>;
    impl<const MAXIMUM: usize> serde::de::Visitor<'_> for Text<MAXIMUM> {
        type Value = String;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(
                formatter,
                "1..{MAXIMUM} bytes of text without control characters"
            )
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if value.is_empty() || value.len() > MAXIMUM || value.chars().any(char::is_control) {
                return Err(E::custom("invalid creature identity text"));
            }
            Ok(value.to_owned())
        }
    }
    deserializer.deserialize_str(Text::<MAXIMUM>)
}
fn read_name<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    read_text::<D, { CreatureIdentity::MAX_NAME_BYTES }>(deserializer)
}
fn read_faction<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    read_text::<D, { CreatureIdentity::MAX_FACTION_BYTES }>(deserializer)
}
impl CreatureIdentity {
    /// Maximum UTF-8 byte length accepted by authoring, spawning and checkpoints.
    pub const MAX_NAME_BYTES: usize = 60;
    /// Maximum UTF-8 byte length of a faction label.
    pub const MAX_FACTION_BYTES: usize = 80;

    fn valid(&self) -> bool {
        valid_identity(&self.name, &self.faction)
    }
}
fn valid_identity(name: &str, faction: &str) -> bool {
    !name.is_empty()
        && name.len() <= CreatureIdentity::MAX_NAME_BYTES
        && !name.chars().any(char::is_control)
        && !faction.is_empty()
        && faction.len() <= CreatureIdentity::MAX_FACTION_BYTES
        && !faction.chars().any(char::is_control)
}

impl crate::Actor {
    /// Discard progress and release its unpaid reservation. Start payments
    /// remain spent; paused/resumed work retains its original hold owner.
    pub(crate) fn cancel_preparation(&mut self) -> Option<crate::combat::Preparation> {
        self.take_preparation(false)
    }

    /// Resolution spends the remainder before effects can kill either actor.
    pub(crate) fn complete_preparation(&mut self) -> Option<crate::combat::Preparation> {
        self.take_preparation(true)
    }

    fn take_preparation(&mut self, completed: bool) -> Option<crate::combat::Preparation> {
        let preparation = self.pending.take()?;
        if let Some(owner) = preparation.origin_intention() {
            if let Some(CombatState { creature, .. }) = self.combat.as_mut() {
                // Free work owns no hold. Avoid copying shared creature state
                // unless a reservation actually changes.
                if creature.costs().reservation(owner).is_some() {
                    if completed {
                        creature
                            .finish_cost(owner)
                            .expect("funded preparation reservation");
                    } else {
                        creature.cancel_cost(owner);
                    }
                }
            }
        }
        Some(preparation)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CombatState {
    /// Derived compatibility view for existing equipment/timing consumers.
    /// Creature checkpoints persist identity and choices, never this cache.
    pub spec: Shared<CombatSpec>,
    creature: Shared<CreatureState>,
}

fn profile(identity: CreatureIdentity, creature: &CreatureState) -> CombatSpec {
    let derived = creature.derived();
    let categories = [
        DamageType::Energy,
        DamageType::Impact,
        DamageType::Keen,
        DamageType::Spirit,
        DamageType::Vital,
    ];
    CombatSpec {
        name: identity.name,
        faction: identity.faction,
        max_hp: derived.maximum_health,
        defense: derived.defenses.physical,
        attack: AttackSpec {
            bonus: i32::from(
                derived.attributes.get(
                    creature
                        .build()
                        .species()
                        .melee
                        .skill()
                        .attribute(creature.build().binding()),
                ),
            ) + i32::from(derived.skills.get(derived.melee.skill()))
                + derived.melee.bonus(),
            wind_up: derived
                .attributes
                .physical_duration(derived.melee.wind_up()),
            recovery: derived
                .attributes
                .physical_duration(derived.melee.recovery()),
            // Natural creature attacks resolve their dice pool in the shared
            // damage kernel; there is no authoritative fixed damage profile.
            damage: BTreeMap::new(),
        },
        immunities: categories
            .into_iter()
            .filter(|&category| derived.protection.immune(category, None))
            .collect::<BTreeSet<_>>(),
        reductions: categories
            .into_iter()
            .filter_map(|category| {
                let amount = derived.protection.reduction(Selector::Category(category));
                (amount != 0).then_some((category, amount))
            })
            .collect(),
    }
}

impl CombatState {
    fn from_creature(identity: CreatureIdentity, creature: CreatureState) -> Self {
        Self {
            spec: Shared::new(profile(identity, &creature)),
            creature: Shared::new(creature),
        }
    }
    pub fn creature(&self) -> &CreatureState {
        &self.creature
    }
    pub fn advance_active(&mut self, ticks: u64) {
        self.creature.advance_active(ticks);
    }
    pub(crate) fn start_preparation_cost(
        &mut self,
        owner: crate::IntentionId,
        cost: crate::costs::ResourceCost,
    ) {
        self.creature
            .start_cost(owner, cost)
            .expect("validated preparation cost");
    }
    pub fn apply_fear(
        &mut self,
        source: ActorId,
        duration: u64,
    ) -> Result<crate::fear::FearUpdate, GameError> {
        self.creature
            .apply_fear(source, duration)
            .map_err(|_| GameError::InvalidLocation)
    }
    pub fn hp(&self) -> u32 {
        self.creature.health().current()
    }
    pub fn valid(&self) -> bool {
        valid_identity(&self.spec.name, &self.spec.faction)
    }
    fn identity(&self) -> CreatureIdentity {
        CreatureIdentity {
            name: self.spec.name.clone(),
            faction: self.spec.faction.clone(),
        }
    }
    pub fn damage(&mut self, amount: u32) -> u32 {
        self.creature.damage(amount).applied
    }
    pub fn heal(&mut self, amount: u32) -> u32 {
        self.creature.heal(amount)
    }
}

#[derive(Serialize)]
struct IdentityRef<'a> {
    name: &'a str,
    faction: &'a str,
}

impl Serialize for CombatState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut record = serializer.serialize_struct("CombatState", 2)?;
        record.serialize_field(
            "identity",
            &IdentityRef {
                name: &self.spec.name,
                faction: &self.spec.faction,
            },
        )?;
        record.serialize_field("creature", &self.creature)?;
        record.end()
    }
}
impl<'de> Deserialize<'de> for CombatState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Record {
            identity: CreatureIdentity,
            creature: CreatureState,
        }
        let record = Record::deserialize(deserializer)?;
        Ok(Self::from_creature(record.identity, record.creature))
    }
}

impl Game {
    /// Configure once, before play. Subsequent changes use injury-preserving rebuilds.
    pub fn configure_creature(
        &mut self,
        id: ActorId,
        identity: CreatureIdentity,
        build: CreatureBuild,
    ) -> Result<(), GameError> {
        if !identity.valid() {
            return Err(GameError::InvalidLocation);
        }
        let creature = CreatureState::new(build).map_err(|_| GameError::InvalidLocation)?;
        if creature.health().dead() {
            return Err(GameError::InvalidLocation);
        }
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        if actor.combat.is_some() || !actor.equipment.is_empty() {
            return Err(GameError::ActorBusy);
        }
        actor.anatomy = Shared::new(creature.derived().anatomy.clone());
        actor.combat = Some(CombatState::from_creature(identity, creature));
        actor.cancel_preparation();
        Ok(())
    }
    /// Read one loaded creature without advancing time, pausing preparation,
    /// evaluating perception or changing caches. Authority belongs to the backend.
    pub fn inspect_creature(&self, id: ActorId) -> Option<CreatureInspection<'_>> {
        let combat = self.actors.get(&id)?.combat.as_ref()?;
        let creature = combat.creature();
        Some(CreatureInspection {
            identity: combat.identity(),
            creature,
            stats: creature.own_stats(),
        })
    }

    pub fn creature(&self, id: ActorId) -> Option<&CreatureState> {
        Some(self.actors.get(&id)?.combat.as_ref()?.creature())
    }
    /// Body-changing transformations are outside this milestone. All supported
    /// transformations preserve sockets, inventory and injury, and run ordinary
    /// death cleanup if their derived maximum falls to the retained injury.
    pub fn rebuild_creature(
        &mut self,
        id: ActorId,
        build: CreatureBuild,
    ) -> Result<RebuildOutcome, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        let combat = actor.combat.as_ref().ok_or(GameError::InvalidLocation)?;
        let mut candidate = combat.creature().clone();
        let mut outcome = candidate
            .rebuild(build)
            .map_err(|_| GameError::InvalidLocation)?;
        if let Some(pending) = actor.pending.as_ref() {
            if let crate::Work::UseAbility { ability, .. } = pending.work {
                if !candidate.derived().abilities.contains(&ability) {
                    if let Some(owner) = pending.origin_intention() {
                        candidate.cancel_cost(owner);
                        if !outcome.canceled.contains(&owner) {
                            outcome.canceled.push(owner);
                        }
                    }
                }
            }
        }
        if candidate.derived().anatomy != *actor.anatomy {
            return Err(GameError::InvalidLocation);
        }
        if outcome.died {
            self.next_item_id
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?;
        }
        let identity = combat.identity();
        let mut actor = self.actors.get_mut(&id).unwrap();
        actor.combat = Some(CombatState::from_creature(identity, candidate));
        let interrupted = actor
            .pending
            .as_ref()
            .filter(|pending| {
                pending
                    .origin_intention()
                    .is_some_and(|id| outcome.canceled.contains(&id))
            })
            .cloned();
        if interrupted.is_some() {
            actor.cancel_preparation();
            actor.ready_at = self.tick;
        }
        drop(actor);
        if outcome.died {
            self.finish_death(id);
        } else if let Some(pending) = interrupted {
            self.combat
                .events
                .push(crate::combat::CombatEvent::Interrupted {
                    actor: id,
                    intention: pending.intention,
                });
            if !self.is_ai(id) {
                self.combat.input_boundaries.insert(id);
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod checkpoint_format_tests {
    use super::*;

    #[test]
    fn raw_combat_profiles_are_rejected_by_the_current_checkpoint_format() {
        let value = serde_json::json!({
            "spec": crate::combat::CombatSpec::default(),
            "hp": 30,
        });
        assert!(serde_json::from_value::<CombatState>(value).is_err());
    }
}

#[cfg(test)]
mod preparation_cost_tests {
    use super::*;
    use crate::attributes::{Attributes, ManaBinding, Skill};
    use crate::costs::ResourceCost;
    use crate::creatures::Species;
    use crate::progression::{Class, CreatureType, HdLedger, HdSource};
    use crate::resources::Resource;
    use crate::{Action, IntentionOrigin};
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

    fn location(x: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        }
    }

    // Install a charge on admitted preparation to exercise the common engine
    // lifecycle before ability action authoring is connected.
    fn charged_game() -> (Game, ActorId, ActorId, crate::IntentionId) {
        charged_game_with_seed(42)
    }

    fn charged_game_with_seed(seed: u64) -> (Game, ActorId, ActorId, crate::IntentionId) {
        let mut game = Game::two_room_in_stone(seed);
        let actor = game
            .spawn_actor(location(1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let target = game
            .spawn_actor(location(2), NonZeroU64::new(10).unwrap())
            .unwrap();
        let build = CreatureBuild::new(
            Species {
                id: "cost_subject".into(),
                kind: CreatureType::Humanoid,
                subtypes: BTreeSet::new(),
                default_attributes: Attributes::default(),
                anatomy: crate::AnatomySpec::humanoid(),
                melee: crate::attacks::MeleeAttack::new(Skill::HeavyWeaponry, 0, 60, 40, {
                    let component = crate::damage::DamageComponent::rolled(
                        crate::combat::DamageType::Impact,
                        None,
                        crate::dice::DicePool::new(1, 4, 0).unwrap(),
                    );
                    let primary = component.key();
                    crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap()
                })
                .unwrap(),
                grants: vec![],
            },
            HdLedger::seeded(vec![HdSource::Class(Class::Warrior)], 7).unwrap(),
            ManaBinding::Intellect,
        )
        .unwrap();
        for id in [actor, target] {
            game.configure_creature(
                id,
                CreatureIdentity {
                    name: "subject".into(),
                    faction: "neutral".into(),
                },
                build.clone(),
            )
            .unwrap();
        }
        let intention = game
            .admit_intention(actor, Action::Attack { target }, IntentionOrigin::Human)
            .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let mut state = game.actors.get_mut(&actor).unwrap();
        let creature = &mut state.combat.as_mut().unwrap().creature;
        creature
            .start_cost(
                intention,
                ResourceCost {
                    resource: Resource::Stamina,
                    start: 1,
                    resolution: 1,
                },
            )
            .unwrap();
        drop(state);
        (game, actor, target, intention)
    }

    #[test]
    fn cancellation_releases_only_the_unpaid_preparation_cost() {
        for teleport in [false, true] {
            let (mut game, actor, _, intention) = charged_game();
            let before = game.clone();
            if teleport {
                game.teleport(actor, location(3)).unwrap();
            } else {
                game.cancel_intention(actor, intention).unwrap();
            }
            let costs = game.creature(actor).unwrap().costs();
            assert!(costs.reservations().is_empty());
            assert_eq!(costs.resources().balance(Resource::Stamina), 1);
            assert_eq!(costs.available(Resource::Stamina), 1);
            assert!(game.preparation(actor).is_none());
            assert_eq!(game.tick(), before.tick());
            assert_eq!(
                before
                    .creature(actor)
                    .unwrap()
                    .costs()
                    .available(Resource::Stamina),
                0,
                "retained snapshots preserve the hold"
            );
        }
    }

    #[test]
    fn completion_pays_the_original_owner_once_after_a_new_resume_admission() {
        let (mut game, actor, target, original) = charged_game();
        game.pause_preparation(actor).unwrap();
        let resumed = game
            .admit_intention(actor, Action::Attack { target }, IntentionOrigin::Human)
            .unwrap();
        assert_ne!(resumed, original);
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(
            game.preparation(actor).unwrap().origin_intention(),
            Some(original)
        );
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Stamina),
            1
        );
        for _ in 0..6 {
            game.act(target, Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 60);
        let costs = game.creature(actor).unwrap().costs();
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.resources().balance(Resource::Stamina), 0);
        assert!(game.preparation(actor).is_none());
    }

    #[test]
    fn a_missed_attack_still_pays_the_resolution_cost() {
        let seed = (0u64..32)
            .find(|&seed| {
                let mut rng = seed;
                crate::dice::roll_check(&mut rng, crate::dice::Edge::default()).kept < 10
            })
            .unwrap();
        let (mut game, actor, target, intention) = charged_game_with_seed(seed);
        let target_health = game.health(target);
        for _ in 0..6 {
            game.act(target, Action::Wait).unwrap();
        }
        assert_eq!(game.health(target), target_health);
        assert!(game.combat.events.iter().any(|event| matches!(event,
            crate::combat::CombatEvent::Resolved { intention: Some(owner), hit: false, .. } if *owner == intention)));
        let costs = game.creature(actor).unwrap().costs();
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.resources().balance(Resource::Stamina), 0);
    }

    #[test]
    fn replacing_preparation_with_new_work_releases_its_original_hold() {
        for moving in [false, true] {
            let (mut game, actor, _, original) = charged_game();
            game.pause_preparation(actor).unwrap();
            if moving {
                game.act(actor, Action::Move(tor_world::Direction::South))
                    .unwrap();
                assert!(game.preparation(actor).is_none());
            } else {
                let mut at = location(1);
                at.position.y = 2;
                let target = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
                game.configure_creature(
                    target,
                    CreatureIdentity {
                        name: "other".into(),
                        faction: "neutral".into(),
                    },
                    game.creature(actor).unwrap().build().clone(),
                )
                .unwrap();
                let replacement = game
                    .admit_intention(actor, Action::Attack { target }, IntentionOrigin::Human)
                    .unwrap();
                assert_ne!(replacement, original);
                assert!(
                    game.creature(actor)
                        .unwrap()
                        .costs()
                        .reservation(original)
                        .is_some(),
                    "queueing does not change payment"
                );
                game.execute_next_intention().unwrap().outcome.unwrap();
                assert_eq!(
                    game.preparation(actor).unwrap().origin_intention(),
                    Some(replacement)
                );
            }
            let costs = game.creature(actor).unwrap().costs();
            assert!(costs.reservations().is_empty());
            assert_eq!(costs.available(Resource::Stamina), 1);
        }
    }

    #[test]
    fn losing_target_reach_releases_the_preparation_hold() {
        let (mut game, actor, target, _) = charged_game();
        game.teleport(target, location(4)).unwrap();
        game.act(target, Action::Wait).unwrap();
        assert!(game.preparation(actor).is_none());
        let costs = game.creature(actor).unwrap().costs();
        assert!(costs.reservations().is_empty());
        assert_eq!(costs.available(Resource::Stamina), 1);
    }
}
