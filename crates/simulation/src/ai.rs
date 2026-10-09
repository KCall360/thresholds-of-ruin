//! Decisions use disclosed observations and remembered navigation, never hidden targets.
use crate::{Action, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_world::{Direction, Location};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiProfile {
    pub memory_ticks: u64,
    pub flee_percent: u32,
}
impl Default for AiProfile {
    fn default() -> Self {
        Self {
            memory_ticks: 1000,
            flee_percent: 25,
        }
    }
}
impl AiProfile {
    pub fn valid(&self) -> bool {
        self.memory_ticks <= 1_000_000 && self.flee_percent <= 100
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    #[default]
    Search,
    Attack,
    Flee,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ai {
    pub profile: AiProfile,
    pub state: State,
    pub target: Option<(ActorId, Location, u64)>,
    #[serde(with = "visit_counts")]
    pub visits: BTreeMap<Location, u64>,
}

// JSON object keys cannot represent a region-local location. Ordered entries also
// let the reader reject duplicate locations instead of silently overwriting them.
mod visit_counts {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        visits: &BTreeMap<Location, u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        visits.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Location, u64>, D::Error> {
        let entries = Vec::<(Location, u64)>::deserialize(deserializer)?;
        let mut visits = BTreeMap::new();
        for (location, count) in entries {
            if visits.insert(location, count).is_some() {
                return Err(serde::de::Error::custom("Duplicate AI visit location"));
            }
        }
        Ok(visits)
    }
}

// Priority-ordered transition table: frightened, target known, resulting state.
const TRANSITIONS: [(bool, bool, State); 4] = [
    (true, true, State::Flee),
    (false, true, State::Attack),
    (true, false, State::Search),
    (false, false, State::Search),
];

impl Game {
    pub fn configure_ai(&mut self, actor: ActorId, profile: AiProfile) -> Result<(), GameError> {
        if !profile.valid() || self.health(actor).is_none() {
            return Err(GameError::InvalidLocation);
        }
        self.combat.ai.insert(
            actor,
            Ai {
                profile,
                state: State::Search,
                target: None,
                visits: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn is_ai(&self, actor: ActorId) -> bool {
        self.combat.ai.contains_key(&actor)
    }

    pub fn next_ai_action(&self) -> Option<(ActorId, Action)> {
        let id = self.next_actor()?;
        self.choose_ai(id).map(|(action, _)| (id, action))
    }

    pub(crate) fn choose_ai(&self, id: ActorId) -> Option<(Action, Ai)> {
        let mut ai = self.combat.ai.get(&id)?.clone();
        let view = self.observe(id).ok()?;
        let visits = ai.visits.entry(view.location).or_default();
        *visits = visits.saturating_add(1);
        let mut search = self.route_search(id).ok();
        let mut route = |destination| search.as_mut()?.route(destination).ok();
        let target = view
            .visible_actors
            .iter()
            .filter(|a| self.hostile(id, a.id))
            .min_by_key(|a| (route(a.location).map_or(usize::MAX, |r| r.len()), a.id));
        if let Some(target) = target {
            ai.target = Some((target.id, target.location, self.tick));
        } else if ai.target.is_some_and(|(_, at, seen)| {
            self.tick.saturating_sub(seen) >= ai.profile.memory_ticks || view.location == at
        }) {
            ai.target = None;
        }
        let (hp, max) = self.health(id)?;
        let frightened = u64::from(hp) * 100 <= u64::from(max) * u64::from(ai.profile.flee_percent);
        ai.state = TRANSITIONS
            .iter()
            .find(|(low, known, _)| *low == frightened && *known == ai.target.is_some())
            .unwrap()
            .2;
        // Use a known restorative before fleeing or attacking. Unknown potions
        // and mixed sequences with harm never qualify as safe healing.
        if u64::from(hp) * 2 <= u64::from(max) {
            if let Some(item) = view
                .inventory
                .iter()
                .find(|item| self.known_healing(id, item.id))
            {
                return Some((Action::Drink { item: item.id }, ai));
            }
        }
        if ai.state != State::Flee && !self.exposed_to_visible_hostile(id, view.location, &view) {
            if let Some(action) = self.choose_gear(id, &view) {
                return Some((action, ai));
            }
            if let Some(action) = self.choose_nearby_loot(id, &view, &mut route) {
                return Some((action, ai));
            }
        }
        if ai.state == State::Flee {
            let (_, threat, _) = ai.target.unwrap();
            let current = route(threat).map_or(0, |r| r.len());
            let mut choices = Vec::new();
            for direction in Direction::HORIZONTAL
                .into_iter()
                .chain([Direction::Up, Direction::Down])
            {
                if let Some((at, _)) = self.actor_translation(id, direction) {
                    if view
                        .visible_cells
                        .iter()
                        .any(|c| c.location == at && !c.wall)
                    {
                        // Distances in the actor's current region; portal retreat uses remembered route length.
                        let score = if at.region == threat.region {
                            (at.position.x - threat.position.x).unsigned_abs() as usize
                                + (at.position.y - threat.position.y).unsigned_abs() as usize
                                + (at.position.z - threat.position.z).unsigned_abs() as usize
                        } else {
                            current + 1
                        };
                        choices.push((score, direction));
                    }
                }
            }
            if let Some((_, direction)) = choices
                .into_iter()
                .filter(|(d, _)| *d > current)
                .max_by_key(|(d, _)| *d)
            {
                return Some((Action::Move(direction), ai));
            }
        }
        if let Some((target, at, _)) = ai.target {
            if self.attack_available(id, target) {
                return Some((Action::Attack { target }, ai));
            }
            if ai.state != State::Flee {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some((Action::Move(step.direction), ai));
                    }
                }
                ai.target = None;
                ai.state = State::Search;
            }
        }
        if ai.state == State::Search {
            if let Some(door) = view
                .visible_cells
                .iter()
                .filter(|c| c.door_reachable)
                .filter_map(|c| c.door)
                .find(|d| !d.open)
            {
                return Some((
                    Action::SetDoor {
                        door: door.id,
                        open: true,
                    },
                    ai,
                ));
            }
            let mut candidates = Vec::new();
            for direction in Direction::HORIZONTAL
                .into_iter()
                .chain([Direction::Up, Direction::Down])
            {
                if let Some((at, _)) = self.actor_translation(id, direction) {
                    if view
                        .visible_cells
                        .iter()
                        .any(|c| c.location == at && !c.wall)
                    {
                        candidates.push((*ai.visits.get(&at).unwrap_or(&0), at, direction));
                    }
                }
            }
            candidates.sort_by_key(|(visits, at, _)| (*visits, *at));
            if let Some((_, _, direction)) = candidates.first() {
                return Some((Action::Move(*direction), ai));
            }
        }
        Some((Action::Wait, ai))
    }

    pub(crate) fn known_healing(&self, actor: ActorId, item: crate::ItemId) -> bool {
        let Some(item) = self.items.get(&item) else {
            return false;
        };
        let Some(actor) = self.actors.get(&actor) else {
            return false;
        };
        (!item.spec.concealed || actor.knowledge.contains(&item.spec.identity))
            && item.spec.consumable.as_ref().is_some_and(|consumable| {
                !consumable.effects.is_empty()
                    && consumable
                        .effects
                        .iter()
                        .all(|effect| matches!(effect, crate::EffectSpec::Heal { amount: 1.. }))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{CombatSpec, DamageType};
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Position, RegionId};
    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }
    fn fixture() -> Game {
        let mut game = Game::two_room_in_stone(42);
        for (x, faction) in [(1, "hero"), (2, "foe")] {
            let id = game
                .spawn_actor(at(1, x, 1), NonZeroU64::new(100).unwrap())
                .unwrap();
            game.configure_combat(
                id,
                CombatSpec {
                    faction: faction.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(
            ActorId(2),
            AiProfile {
                memory_ticks: 50,
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        game
    }
    #[test]
    fn one_decision_shares_its_search_across_visible_targets() {
        let mut game = fixture();
        let other = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_combat(
            other,
            CombatSpec {
                faction: "hero".into(),
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        assert_eq!(game.observe(ActorId(2)).unwrap().visible_actors.len(), 2);
        let before = crate::diagnostics::work_counts().route_searches;
        let (action, _) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(action, Action::Attack { target: ActorId(1) });
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
    }

    #[test]
    fn hidden_movement_does_not_update_target_memory_and_memory_expires() {
        let mut game = fixture();
        let (_, ai) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.target, Some((ActorId(1), at(1, 1, 1), 0)));
        game.combat.ai.insert(ActorId(2), ai);
        game.set_region_light(RegionId(2), false).unwrap();
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let (_, remembered) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(remembered.target, Some((ActorId(1), at(1, 1, 1), 0)));
        game.tick = 50;
        let (_, expired) = game.choose_ai(ActorId(2)).unwrap();
        assert!(expired.target.is_none());
        assert_eq!(expired.state, State::Search);
    }
    #[test]
    fn wounded_ai_flees_and_a_cornered_ai_fights() {
        let mut game = fixture();
        game.apply_damage(ActorId(2), &BTreeMap::from([(DamageType::Vital, 25)]));
        let (action, ai) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert!(matches!(action, Action::Move(_)));
        for (x, y) in [(1, 0), (1, 2), (2, 0), (2, 2), (3, 0), (3, 1), (3, 2)] {
            game.set_wall(at(1, x, y), true).unwrap();
        }
        let (action, ai) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert_eq!(action, Action::Attack { target: ActorId(1) });
    }

    fn ring(game: &mut Game, item: u64, defense: i32, concealed: bool) {
        let mut spec = crate::ItemSpec::ordinary(format!("ring-{item}"));
        spec.class = crate::ItemClass::Ring;
        spec.concealed = concealed;
        spec.appearance = "silver ring".into();
        spec.equipment = Some(crate::EquipmentSpec {
            slot: crate::EquipmentSlot::Ring,
            attack: None,
            defense,
            reductions: BTreeMap::new(),
        });
        game.place_item_stack(item, at(1, 2, 1), Some(ActorId(2)), 1, spec)
            .unwrap();
    }

    #[test]
    fn loot_range_and_safe_first_step_are_checked_on_remembered_routes() {
        use tor_world::{Extent, Region, World};
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(Region {
                id: RegionId(1),
                name: "arena".into(),
                bounds: Extent::new(10, 5, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42);
        let human = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let actor = game
            .spawn_actor(at(1, 1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        for (id, faction) in [(human, "hero"), (actor, "foe")] {
            game.configure_combat(
                id,
                CombatSpec {
                    faction: faction.into(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        game.configure_run(
            human,
            BTreeSet::from([human]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            effects: vec![crate::EffectSpec::Heal { amount: 10 }],
        });
        game.place_item_stack(20, at(1, 4, 2), None, 1, potion.clone())
            .unwrap();
        game.place_item_stack(21, at(1, 1, 0), None, 1, potion.clone())
            .unwrap();
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        let mut search = game.route_search(actor).unwrap();
        let mut route = |at| search.route(at).ok();
        // The eastward first step is adjacent to the hostile; the northward
        // alternative remains safe and must still be considered.
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &mut route),
            Some(Action::Move(Direction::North))
        );
        game.teleport(human, at(1, 8, 2)).unwrap();
        game.place_item_stack(22, at(1, 5, 2), None, 1, potion)
            .unwrap();
        game.refresh_navigation();
        let mut view = game.observe(actor).unwrap();
        view.ground_items
            .retain(|item| item.id == crate::ItemId(22));
        let mut search = game.route_search(actor).unwrap();
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &mut |at| search.route(at).ok()),
            None
        );
    }

    #[test]
    fn nearby_loot_uses_own_knowledge_and_takes_only_one_unit() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.stackable = true;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
            effects: vec![crate::EffectSpec::Heal { amount: 10 }],
        });
        game.place_item_stack(20, at(1, 2, 1), None, 3, potion)
            .unwrap();
        game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
        assert!(!matches!(
            game.choose_ai(actor).unwrap().0,
            Action::Take { .. }
        ));
        game.identify_item(actor, crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Take {
                item: crate::ItemId(20),
                quantity: Some(1)
            }
        );
    }

    #[test]
    fn nearby_loot_routes_share_one_search_and_do_not_step_beside_hostiles() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(1, 5, 1)).unwrap();
        for (id, x) in [(20, 3), (21, 4)] {
            let mut potion = crate::ItemSpec::ordinary(format!("healing {id}"));
            potion.class = crate::ItemClass::Potion;
            potion.consumable = Some(crate::ConsumableSpec {
                effects: vec![crate::EffectSpec::Heal { amount: 10 }],
            });
            game.place_item_stack(id, at(1, x, 1), None, 1, potion)
                .unwrap();
        }
        game.refresh_navigation();
        let before = crate::diagnostics::work_counts().route_searches;
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Move(Direction::East)
        );
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
        game.teleport(actor, at(1, 3, 1)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Take {
                item: crate::ItemId(20),
                quantity: Some(1)
            }
        );
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Attack { target: ActorId(1) }
        );
    }

    #[test]
    fn equipment_uses_duplicate_anatomy_sockets_and_only_the_ais_known_stats() {
        let mut game = fixture();
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.configure_anatomy(
            ActorId(2),
            crate::AnatomySpec {
                slots: vec![crate::EquipmentSlot::Ring; 2],
            },
        )
        .unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        ring(&mut game, 12, 3, true);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        let expected = Action::Equip {
            item: crate::ItemId(11),
            slot: crate::EquipmentSlotId(1),
        };
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().0, expected);
        game.identify_item(ActorId(1), crate::ItemId(12)).unwrap();
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().0, expected);
        game.identify_item(ActorId(2), crate::ItemId(12)).unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().0,
            Action::Equip {
                item: crate::ItemId(12),
                slot: crate::EquipmentSlotId(1)
            }
        );
    }

    #[test]
    fn gear_removal_chooses_an_upgrade_and_does_not_reequip_the_dominated_old_item() {
        let mut game = fixture();
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.configure_anatomy(
            ActorId(2),
            crate::AnatomySpec {
                slots: vec![crate::EquipmentSlot::Ring],
            },
        )
        .unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().0,
            Action::Unequip {
                item: crate::ItemId(10)
            }
        );
        game.actors.get_mut(&ActorId(2)).unwrap().equipment.clear();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().0,
            Action::Equip {
                item: crate::ItemId(11),
                slot: crate::EquipmentSlotId(0)
            }
        );
    }

    #[test]
    fn gear_work_does_not_replace_unknown_equipment_or_start_beside_a_visible_hostile() {
        let mut game = fixture();
        game.configure_anatomy(
            ActorId(2),
            crate::AnatomySpec {
                slots: vec![crate::EquipmentSlot::Ring],
            },
        )
        .unwrap();
        ring(&mut game, 10, 1, true);
        ring(&mut game, 11, 2, false);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().0,
            Action::Attack { target: ActorId(1) }
        );
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        assert!(!matches!(
            game.choose_ai(ActorId(2)).unwrap().0,
            Action::Unequip { .. } | Action::Equip { .. }
        ));
    }

    #[test]
    fn healing_decisions_require_the_ai_actors_own_identity_knowledge() {
        let mut game = fixture();
        let actor = ActorId(2);
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
            effects: vec![crate::EffectSpec::Heal { amount: 10 }],
        });
        game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
            .unwrap();
        game.apply_damage(actor, &BTreeMap::from([(DamageType::Vital, 15)]));
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Attack { target: ActorId(1) }
        );
        game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Attack { target: ActorId(1) }
        );
        game.identify_item(actor, crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Drink {
                item: crate::ItemId(20)
            }
        );
    }

    #[test]
    fn healing_priority_does_not_drink_known_mixed_harmful_effects_or_waste_full_health() {
        let mut game = fixture();
        let actor = ActorId(2);
        let mut potion = crate::ItemSpec::ordinary("mixed".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            effects: vec![
                crate::EffectSpec::Heal { amount: 10 },
                crate::EffectSpec::Damage {
                    components: BTreeMap::from([(DamageType::Vital, 1)]),
                },
            ],
        });
        game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
            .unwrap();
        game.apply_damage(actor, &BTreeMap::from([(DamageType::Vital, 15)]));
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Attack { target: ActorId(1) }
        );
        let mut healing = crate::ItemSpec::ordinary("healing".into());
        healing.class = crate::ItemClass::Potion;
        healing.consumable = Some(crate::ConsumableSpec {
            effects: vec![crate::EffectSpec::Heal { amount: 20 }],
        });
        game.place_item_stack(21, at(1, 2, 1), Some(actor), 1, healing)
            .unwrap();
        game.apply_effects(actor, &[crate::EffectSpec::Heal { amount: 30 }])
            .unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Attack { target: ActorId(1) }
        );
        game.apply_damage(actor, &BTreeMap::from([(DamageType::Vital, 25)]));
        assert_eq!(
            game.choose_ai(actor).unwrap().0,
            Action::Drink {
                item: crate::ItemId(21)
            }
        );
    }
}
