//! Item decisions inspect definitions only after the acting actor knows them.
use crate::{
    Action, ActorId, EquipmentSlotId, EquipmentSpec, Game, ItemId, Observation, TravelStep,
};
use tor_world::Location;

impl Game {
    pub(crate) fn choose_known_gold(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        if !profile.gold {
            return None;
        }
        view.ground_items
            .iter()
            .filter(|item| item.known_currency == Some(crate::CurrencyKind::Gold))
            .filter(|item| !self.exposed_to_visible_hostile(actor, item.location, view))
            .filter_map(|item| {
                if profile.idle == crate::behavior::IdleBehavior::Wait
                    && item.location != view.location
                {
                    return None;
                }
                let path = if item.location == view.location {
                    vec![]
                } else {
                    route(item.location)?
                };
                if path.len() > 3 {
                    return None;
                }
                let action = if let Some(step) = path.first() {
                    let (at, _) = self.actor_translation(actor, step.direction)?;
                    if self.exposed_to_visible_hostile(actor, at, view) {
                        return None;
                    }
                    Action::Move(step.direction)
                } else {
                    Action::Take {
                        item: item.id,
                        quantity: Some(item.quantity),
                    }
                };
                (!self.condition_blocks_action(actor, &action)).then_some((
                    path.len(),
                    item.id,
                    action,
                ))
            })
            .min_by_key(|(distance, id, _)| (*distance, *id))
            .map(|(_, _, action)| action)
    }

    pub(crate) fn choose_nearby_loot(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        sight: &[tor_world::SightCell],
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        if view.ground_items.is_empty() {
            return None;
        }
        let has_healing = view
            .inventory
            .iter()
            .any(|item| self.known_healing(actor, item.id));
        let supplies = if profile.care
            && view
                .ground_items
                .iter()
                .any(|item| item.known_supply.is_some())
        {
            self.healing_supply_reserve(actor, sight)
        } else {
            Default::default()
        };
        let wants_food = profile.care
            && required_nutrition(view).is_some()
            && !view
                .inventory
                .iter()
                .any(|item| self.known_food(actor, item.id));
        view.ground_items
            .iter()
            .filter_map(|item| {
                let useful_gear =
                    profile.equipment && self.gear_destination(actor, item.id).is_some();
                let healing = profile.care && !has_healing && self.known_healing(actor, item.id);
                let food = wants_food && self.known_food(actor, item.id);
                let supply = profile
                    .care
                    .then(|| {
                        let id = &item.known_supply.as_ref()?.id;
                        let required = *supplies.get(id)?;
                        let carried = view
                            .inventory
                            .iter()
                            .filter(|owned| {
                                owned
                                    .known_supply
                                    .as_ref()
                                    .is_some_and(|known| &known.id == id)
                            })
                            .fold(0_u64, |sum, owned| sum.saturating_add(owned.quantity));
                        required
                            .checked_sub(carried)
                            .filter(|quantity| *quantity > 0)
                    })
                    .flatten();
                if useful_gear || healing || food {
                    Some((item, 1))
                } else {
                    supply.map(|quantity| (item, quantity.min(item.quantity)))
                }
            })
            .filter(|(item, _)| !self.exposed_to_visible_hostile(actor, item.location, view))
            .filter_map(|(item, quantity)| {
                if profile.idle == crate::behavior::IdleBehavior::Wait
                    && item.location != view.location
                {
                    return None;
                }
                // Present underfoot contact needs no remembered route. A newly
                // spawned actor may not yet have a navigation record at all.
                let path = if item.location == view.location {
                    Vec::new()
                } else {
                    route(item.location)?
                };
                if path.len() > 3 {
                    return None;
                }
                let action = if let Some(step) = path.first() {
                    let (at, _) = self.actor_translation(actor, step.direction)?;
                    if self.exposed_to_visible_hostile(actor, at, view) {
                        return None;
                    }
                    Action::Move(step.direction)
                } else {
                    Action::Take {
                        item: item.id,
                        quantity: Some(quantity),
                    }
                };
                if self.condition_blocks_action(actor, &action) {
                    return None;
                }
                Some((path.len(), item.id, action))
            })
            .min_by_key(|(distance, item, _)| (*distance, *item))
            .map(|(_, _, action)| action)
    }

    pub(crate) fn known_food(&self, actor: ActorId, item: ItemId) -> bool {
        self.items.get(&item).is_some_and(|item| {
            item.spec.edible.is_some()
                && (!item.spec.concealed
                    || self
                        .actors
                        .get(&actor)
                        .is_some_and(|owner| owner.knowledge.contains(&item.spec.identity)))
        })
    }

    /// One ordinary use of an owned, exclusively restorative technique. Names,
    /// hidden piles and other actors' claims never grant an interest in supplies.
    fn healing_supply_reserve(
        &self,
        actor: ActorId,
        sight: &[tor_world::SightCell],
    ) -> std::collections::BTreeMap<crate::content_ids::SupplyId, u64> {
        let Some(creature) = self.creature(actor) else {
            return Default::default();
        };
        let rules = creature.build().ruleset();
        let mut choices: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                rules
                    .ability(id)
                    .map(|definition| (definition.ai.priority, id, definition))
            })
            .collect();
        choices.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, definition) in choices {
            if definition.ai.hostile || definition.supply_costs.is_empty() {
                continue;
            }
            if let Some(cost) = &definition.cost {
                let available = creature.costs().available(cost.resource);
                let maximum = creature.costs().resources().maximum(cost.resource);
                if cost
                    .start
                    .checked_add(cost.resolution)
                    .is_none_or(|total| total > available)
                    || u64::from(available) * 100
                        < u64::from(maximum) * u64::from(definition.ai.minimum_resource_percent)
                {
                    continue;
                }
            }
            // Compilation resolves self reach, cooldown, unlocked strikes and
            // effective operands without allocating physical supply claims.
            let Ok(plan) = self.ability_plan_in_scene(actor, id, actor, sight) else {
                continue;
            };
            if !self.ai_plan_resources_available(&plan) || !crate::ai::safe_healing_plan(&plan) {
                continue;
            }
            // Keep one complete recipe; component-wise minima across alternatives
            // could leave an inventory able to pay for neither.
            let mut required = std::collections::BTreeMap::<_, u64>::new();
            for cost in &definition.supply_costs {
                let count = required.entry(cost.supply.clone()).or_default();
                *count = count.saturating_add(cost.quantity);
            }
            return required;
        }
        Default::default()
    }

    fn known_gear(&self, actor: ActorId, item: ItemId) -> Option<EquipmentSpec> {
        let mut estimate = self.items.get(&item)?.clone();
        estimate.condition = self.gear_condition_estimate(actor, item)?;
        estimate.equipment_view()
    }

    /// Private planning estimate, never a disclosed numerical item view. An
    /// identified archetype supplies its ordinary baseline until this actor has
    /// observed exact instance numbers; hidden mutations never refresh memory.
    fn gear_condition_estimate(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<crate::armor::ItemCondition> {
        let owner = self.actors.get(&actor)?;
        let definition = &self.items.get(&item)?.spec;
        if definition.concealed && !owner.knowledge.contains(&definition.identity) {
            return None;
        }
        definition.equipment.as_ref()?;
        Some(
            owner
                .known_equipment
                .remembered_condition(item)
                .unwrap_or_default(),
        )
    }

    pub(crate) fn exposed_to_visible_hostile(
        &self,
        actor: ActorId,
        at: Location,
        view: &Observation,
    ) -> bool {
        view.visible_actors
            .iter()
            .filter(|other| self.perceived_hostile(actor, other.id, other.disclosure))
            .any(|other| {
                other.location == at
                    || (-1..=1).any(|x| {
                        (-1..=1).any(|y| {
                            (-1..=1).any(|z| {
                                [x, y, z] != [0, 0, 0]
                                    && self.melee_neighbor(other.location, [x, y, z]) == Some(at)
                            })
                        })
                    })
            })
    }

    pub(crate) fn gear_destination(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<(EquipmentSlotId, Option<ItemId>)> {
        let candidate = self.known_gear(actor, item)?;
        let owner = self.actors.get(&actor)?;
        if owner.equipment.values().any(|equipped| *equipped == item) {
            return None;
        }
        let mut sockets: Vec<_> = owner.anatomy.slots.iter().enumerate().collect();
        sockets.sort_by_key(|(index, _)| {
            (
                owner
                    .equipment
                    .contains_key(&EquipmentSlotId(*index as u16)),
                *index,
            )
        });
        for (index, kind) in sockets {
            if *kind != candidate.slot {
                continue;
            }
            let slot = EquipmentSlotId(u16::try_from(index).ok()?);
            let old = owner.equipment.get(&slot).copied();
            if old.is_some_and(|old| {
                self.known_gear(actor, old).is_none()
                    || owner.known_equipment.curse(old) == Some(true)
            }) {
                continue;
            }
            if self.gear_upgrade(actor, item, slot) {
                return Some((slot, old));
            }
        }
        None
    }

    /// Candidate evaluation uses the production modifier fold. It changes only
    /// a private snapshot; the chosen ordinary action still owns admission.
    fn gear_projection(&self, actor: ActorId, item: ItemId, slot: EquipmentSlotId) -> Option<Game> {
        let mut equipment = self.actors.get(&actor)?.equipment.clone();
        equipment.insert(slot, item);
        self.known_loadout_projection(actor, equipment)
    }

    /// Baseline and candidates use the same remembered facts or ordinary
    /// archetype estimates. Reading live hidden condition would make AI an oracle.
    fn known_loadout_projection(
        &self,
        actor: ActorId,
        equipment: std::collections::BTreeMap<EquipmentSlotId, ItemId>,
    ) -> Option<Game> {
        let mut candidate = self.clone();
        for id in equipment.values() {
            let condition = self.gear_condition_estimate(actor, *id)?;
            candidate
                .items
                .edit(*id, |item| item.condition = condition)?;
        }
        let sources = equipment
            .iter()
            .map(|(socket, id)| {
                candidate
                    .items
                    .get(id)?
                    .equipment_source(*id, *socket, &candidate.content.rules)
            })
            .collect::<Option<Vec<_>>>()?;
        let context = candidate.equipment_recovery_context(actor, &sources).ok()?;
        let mut owner = candidate.actors.get_mut(&actor)?;
        let combat = owner.combat.as_mut()?;
        let result = combat
            .project_equipment_immediately_in_context(sources, context)
            .ok()?;
        if result.died {
            return None;
        }
        owner.equipment = equipment;
        drop(owner);
        Some(candidate)
    }

    fn gear_upgrade(&self, actor: ActorId, item: ItemId, slot: EquipmentSlotId) -> bool {
        let Some(candidate) = self.gear_projection(actor, item, slot) else {
            return false;
        };
        let before = self
            .actors
            .get(&actor)
            .and_then(|owner| self.known_loadout_projection(actor, owner.equipment.clone()));
        let (Some(before), Some(after)) = (
            before.as_ref().and_then(|game| gear_score(game, actor)),
            gear_score(&candidate, actor),
        ) else {
            return false;
        };
        after.dominates(&before)
    }

    pub(crate) fn choose_gear(&self, actor: ActorId, view: &Observation) -> Option<Action> {
        let choices: Vec<_> = view
            .inventory
            .iter()
            .filter_map(|item| {
                self.gear_destination(actor, item.id)
                    .map(|destination| (item.id, destination))
            })
            .collect();
        // Compare complete prospective loadouts, including item interactions,
        // rather than folding a parallel set of equipment-only stat formulas.
        let (item, (slot, old)) = choices.iter().find(|(item, (slot, _))| {
            let Some(candidate) = self
                .gear_projection(actor, *item, *slot)
                .and_then(|game| gear_score(&game, actor))
            else {
                return false;
            };
            !choices.iter().any(|(other, (other_slot, _))| {
                self.gear_projection(actor, *other, *other_slot)
                    .and_then(|game| gear_score(&game, actor))
                    .is_some_and(|other| other.dominates(&candidate))
            })
        })?;
        Some(old.map_or(
            Action::Equip {
                item: *item,
                slot: *slot,
            },
            |item| Action::Unequip { item },
        ))
    }
}

pub(crate) fn required_nutrition(view: &Observation) -> Option<crate::nutrition::Hunger> {
    match view.combat.as_ref()?.own_stats.as_ref()?.nutrition {
        crate::nutrition::NutritionView::Required { hunger, .. } => Some(hunger),
        crate::nutrition::NutritionView::Exempt => None,
    }
}
#[derive(PartialEq, Eq)]
struct GearScore {
    maximum_health: u64,
    stats: std::collections::BTreeMap<crate::rules::Stat, i64>,
    flags: std::collections::BTreeSet<crate::content_ids::FlagId>,
    abilities: std::collections::BTreeSet<crate::content_ids::AbilityId>,
    protection: std::collections::BTreeMap<crate::rules::Selector, (bool, u32)>,
    attack: AttackScore,
}
#[derive(PartialEq, Eq)]
struct AttackScore {
    bonus: i64,
    prepare: u64,
    recovery: u64,
    damage: std::collections::BTreeMap<
        (
            crate::combat::DamageType,
            Option<crate::content_ids::DescriptorId>,
        ),
        i64,
    >,
}
impl GearScore {
    fn dominates(&self, old: &Self) -> bool {
        let improved = self
            .stats
            .iter()
            .any(|(stat, value)| *value > old.stats.get(stat).copied().unwrap_or(0))
            || self.maximum_health > old.maximum_health
            || self.flags.len() > old.flags.len()
            || self.abilities.len() > old.abilities.len()
            || self
                .protection
                .iter()
                .any(|(selector, (immune, reduction))| {
                    let (old_immune, old_reduction) =
                        old.protection.get(selector).copied().unwrap_or_default();
                    (*immune && !old_immune) || (!old_immune && *reduction > old_reduction)
                })
            || self.attack.bonus > old.attack.bonus
            || self.attack.prepare < old.attack.prepare
            || self.attack.recovery < old.attack.recovery
            || self
                .attack
                .damage
                .iter()
                .any(|(key, value)| *value > old.attack.damage.get(key).copied().unwrap_or(0));
        improved
            && self.maximum_health >= old.maximum_health
            && old
                .stats
                .iter()
                .all(|(stat, value)| self.stats.get(stat).copied().unwrap_or(0) >= *value)
            && self
                .stats
                .iter()
                .all(|(stat, value)| *value >= old.stats.get(stat).copied().unwrap_or(0))
            && self.flags.is_superset(&old.flags)
            && self.abilities.is_superset(&old.abilities)
            && old
                .protection
                .iter()
                .all(|(selector, (immune, reduction))| {
                    let (new_immune, new_reduction) =
                        self.protection.get(selector).copied().unwrap_or_default();
                    (!immune || new_immune) && (new_immune || new_reduction >= *reduction)
                })
            && self.attack.bonus >= old.attack.bonus
            && self.attack.prepare <= old.attack.prepare
            && self.attack.recovery <= old.attack.recovery
            && old
                .attack
                .damage
                .iter()
                .all(|(key, amount)| self.attack.damage.get(key).copied().unwrap_or(0) >= *amount)
    }
}

/// Read cache values only. Every candidate above was derived by the same source
/// fold as actual equipment; damage estimates never inspect enemy protection.
fn gear_score(game: &Game, actor: ActorId) -> Option<GearScore> {
    use crate::rules::{AbilityStat, DefenseKind, ModifierOp, Stat};
    let creature = game.creature(actor)?;
    let derived = creature.derived();
    let mut stats = std::collections::BTreeMap::new();
    for attribute in crate::attributes::Attribute::ALL {
        stats.insert(
            Stat::Attribute { attribute },
            i64::from(derived.attributes.get(attribute)),
        );
    }
    for (defense, value) in [
        (DefenseKind::Physical, derived.defenses.physical),
        (DefenseKind::Cognitive, derived.defenses.cognitive),
        (DefenseKind::Spiritual, derived.defenses.spiritual),
    ] {
        stats.insert(Stat::Defense { defense }, i64::from(value));
    }
    // Lower derived periods/reset percentages are improvements. Comparing final
    // values also accounts for HD scaling and all ordinary source interactions.
    stats.insert(
        Stat::HealthRecoveryPeriod,
        -(derived.health_recovery_period as i64),
    );
    stats.insert(
        Stat::HealthRecoveryResetPercent,
        -i64::from(derived.health_recovery_reset_percent),
    );
    for resource in crate::resources::Resource::ALL {
        stats.insert(
            Stat::ResourceMax { resource },
            i64::from(creature.costs().resources().maximum(resource)),
        );
    }
    for (ability, values) in &derived.ability_values {
        for (stat, value) in [
            (AbilityStat::Dice, i64::from(values.dice)),
            (AbilityStat::FlatBonus, i64::from(values.flat)),
            (AbilityStat::CheckBonus, i64::from(values.check_bonus)),
            (AbilityStat::Difficulty, i64::from(values.difficulty)),
            (AbilityStat::Duration, values.duration as i64),
            (AbilityStat::Range, i64::from(values.range)),
            (AbilityStat::Preparation, -values.preparation),
            (AbilityStat::Recovery, -values.recovery),
        ] {
            stats.insert(
                Stat::Ability {
                    ability: ability.clone(),
                    stat,
                },
                value,
            );
        }
    }
    let mut protection = std::collections::BTreeMap::new();
    for modifier in derived.sources.values().flatten() {
        if let ModifierOp::Immunity { selector } | ModifierOp::Reduction { selector, .. } =
            &modifier.op
        {
            protection.insert(
                selector.clone(),
                (
                    derived.protection.has_immunity(selector),
                    derived.protection.reduction(selector),
                ),
            );
        }
    }
    let attack = game.selected_melee_attack(actor)?;
    let basic = game.basic_attack(actor)?;
    let values = derived.ability_values(&basic);
    let damage = match attack.damage_with_modifiers(values.dice, values.flat, 0) {
        Ok(damage) => Some(damage),
        Err(crate::attacks::SelectedDamageError::NoPrimaryDamage) => None,
        Err(crate::attacks::SelectedDamageError::InvalidDefinition) => return None,
    };
    let mut estimates = std::collections::BTreeMap::new();
    for component in damage.iter().flat_map(|damage| damage.components()) {
        let estimate = match component.amount() {
            crate::damage::DamageAmount::Fixed(value) => i64::from(value) * 2,
            crate::damage::DamageAmount::Rolled(pool) => (i64::from(pool.count())
                * (i64::from(pool.sides()) + 1)
                + i64::from(pool.bonus()) * 2)
                .max(0),
        };
        let key = component.key();
        *estimates.entry((key.category, key.descriptor)).or_insert(0) += estimate;
    }
    let bonus = derived.selected_attack.check_bonus;
    // Candidate scoring consumes the same cached timing as the ability planner.
    let timing = game.ability_timing(actor, &basic).ok()?;
    Some(GearScore {
        maximum_health: derived.maximum_health,
        stats,
        flags: derived.flags.clone(),
        abilities: derived.abilities.clone(),
        protection,
        attack: AttackScore {
            bonus,
            prepare: timing.preparation,
            recovery: timing.recovery,
            damage: estimates,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EquipmentSlot;
    use crate::{
        combat::DamageType,
        content_ids::DescriptorId,
        rules::{DefenseKind, Modifier, ModifierOp, Scope, Selector, Stat},
    };

    fn fixture() -> (Game, ActorId, Location) {
        let mut species = crate::test_creatures::species();
        species.default_attributes =
            crate::attributes::Attributes::new(crate::attributes::ByAttribute {
                strength: 5,
                speed: 1,
                ..Default::default()
            })
            .unwrap();
        let mut game = Game::two_room_in_stone(
            42,
            crate::test_creatures::content_with(species.clone(), vec![]),
        );
        let at = Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 1, z: 0 },
        };
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(&mut game, actor, "neutral", species);
        (game, actor, at)
    }

    fn gold_fixture() -> (Game, ActorId, Location) {
        let (mut game, actor, at) = fixture();
        game.configure_ai(
            actor,
            crate::ai::AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                gold: true,
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        (game, actor, at)
    }

    fn gold(concealed: bool) -> crate::ItemSpec {
        let mut item = crate::ItemSpec::ordinary("gold coin".into());
        item.class = crate::ItemClass::Coin;
        item.stackable = true;
        item.currency = Some(crate::CurrencyKind::Gold);
        item.concealed = concealed;
        item
    }

    fn healing_potion() -> crate::ItemSpec {
        let mut item = crate::ItemSpec::ordinary("known restorative".into());
        item.class = crate::ItemClass::Potion;
        item.consumable = Some(crate::ConsumableSpec {
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![crate::rules::Effect::Heal {
                amount: crate::rules::AmountExpr::Fixed { amount: 2 },
            }],
        });
        item
    }

    fn neighbor(game: &mut Game, actor: ActorId, at: Location) -> ActorId {
        let build = game.creature(actor).unwrap().build().clone();
        let other = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            other,
            crate::CreatureIdentity {
                name: "neutral neighbor".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        game.refresh_navigation();
        other
    }

    #[test]
    fn known_gold_precedes_idle_without_enabling_other_loot_policies() {
        let (mut game, actor, at) = gold_fixture();
        game.place_item_stack(19, at, None, 1, healing_potion())
            .unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
        let profile = &game.combat.ai[&actor].profile;
        assert!(!profile.care && !profile.equipment && !profile.loot && !profile.doors);
        game.place_item_stack(20, at, None, 7, gold(false)).unwrap();
        let view = game.observe(actor).unwrap();
        assert_eq!(
            view.ground_items
                .iter()
                .find(|item| item.id == ItemId(20))
                .unwrap()
                .known_currency,
            Some(crate::CurrencyKind::Gold)
        );
        let before = game.clone();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: ItemId(20),
                quantity: Some(7)
            }
        );
        assert_eq!(
            game, before,
            "A preview must not move gold or advance its RNG"
        );
        game.combat.ai.get_mut(&actor).unwrap().profile.gold = false;
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
    }

    #[test]
    fn known_gold_uses_a_disclosed_short_route_only_when_idle_policy_allows_it() {
        let (mut game, actor, at) = gold_fixture();
        let destination = Location {
            position: tor_world::Position {
                x: at.position.x + 2,
                ..at.position
            },
            ..at
        };
        game.place_item_stack(20, destination, None, 7, gold(false))
            .unwrap();
        game.refresh_navigation();
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
        game.combat.ai.get_mut(&actor).unwrap().profile.idle =
            crate::behavior::IdleBehavior::Explore;
        let before = game.clone();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(tor_world::Direction::East)
        );
        assert_eq!(game, before);
    }

    #[test]
    fn concealed_gold_and_another_actors_private_purse_do_not_change_idle_choice() {
        let (mut game, actor, at) = gold_fixture();
        let other_at = Location {
            position: tor_world::Position {
                x: at.position.x + 1,
                ..at.position
            },
            ..at
        };
        let other = neighbor(&mut game, actor, other_at);
        let mut hidden = game.clone();
        hidden
            .place_item_stack(21, other_at, Some(other), 99, gold(false))
            .unwrap();
        assert_eq!(game.observe(actor).unwrap(), hidden.observe(actor).unwrap());
        assert_eq!(hidden.choose_ai(actor).unwrap().action, Action::Wait);
        let mut ordinary_lookalike = game.clone();
        game.place_item_stack(20, at, None, 7, gold(true)).unwrap();
        let mut unknown_non_currency = gold(true);
        unknown_non_currency.currency = None;
        ordinary_lookalike
            .place_item_stack(20, at, None, 7, unknown_non_currency)
            .unwrap();
        assert_eq!(
            game.observe(actor).unwrap(),
            ordinary_lookalike.observe(actor).unwrap()
        );
        assert_eq!(game.choose_ai(actor).unwrap().action, Action::Wait);
        assert_eq!(
            ordinary_lookalike.choose_ai(actor).unwrap().action,
            Action::Wait
        );
        game.identify_item(actor, ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: ItemId(20),
                quantity: Some(7)
            }
        );
    }

    #[test]
    fn healing_retaliation_and_escape_retain_priority_over_visible_gold() {
        let (mut game, actor, at) = gold_fixture();
        game.place_item_stack(20, at, None, 7, gold(false)).unwrap();
        let mut wounded = game.clone();
        wounded.combat.ai.get_mut(&actor).unwrap().profile.care = true;
        wounded
            .place_item_stack(21, at, Some(actor), 1, healing_potion())
            .unwrap();
        let hp = wounded.health(actor).unwrap().0;
        crate::test_creatures::injure(&mut wounded, actor, u32::try_from(hp - 1).unwrap()).unwrap();
        assert_eq!(
            wounded.choose_ai(actor).unwrap().action,
            Action::Drink { item: ItemId(21) }
        );

        let other_at = Location {
            position: tor_world::Position {
                x: at.position.x + 1,
                ..at.position
            },
            ..at
        };
        let other = neighbor(&mut game, actor, other_at);
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: ItemId(20),
                quantity: Some(7)
            },
            "A neutral visible neighbor alone is not provocation"
        );
        let basic = crate::content_ids::AbilityId::new("basic_melee").unwrap();
        let plan = game.ability_plan(other, &basic, actor).unwrap();
        let outcome = game
            .resolve_strike(
                plan.as_ability().unwrap(),
                0,
                None,
                &mut crate::combat::PreparationProgress::Ability {
                    next_strike: 1,
                    successful_primaries: 0,
                    attempted_riders: [0; 2],
                    linked_hold: None,
                },
            )
            .unwrap();
        assert!(outcome.target_participated());
        let retaliation = game.choose_ai(actor).unwrap();
        assert_eq!(retaliation.ai.state, crate::ai::State::Attack);
        assert_eq!(
            retaliation.action,
            Action::UseAbility {
                id: basic,
                invocation: crate::abilities::AbilityInvocation::actor_target(other),
                supplies: Default::default(),
            }
        );
        let hp = game.health(actor).unwrap().0;
        crate::test_creatures::injure(&mut game, actor, u32::try_from(hp - 1).unwrap()).unwrap();
        let escape = game.choose_ai(actor).unwrap();
        assert_eq!(escape.ai.state, crate::ai::State::Flee);
        assert!(matches!(escape.action, Action::Move(_)));
    }

    fn add(game: &mut Game, actor: ActorId, at: Location, id: u64, gear: EquipmentSpec) {
        let mut item = crate::ItemSpec::ordinary(format!("gear {id}"));
        item.class = if gear.slot == EquipmentSlot::new("weapon").unwrap() {
            crate::ItemClass::Weapon
        } else {
            crate::ItemClass::Armor
        };
        item.equipment = Some(gear);
        game.place_item_stack(id, at, Some(actor), 1, item).unwrap();
    }
    fn defense(value: i64) -> Modifier {
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Defense {
                    defense: DefenseKind::Cognitive,
                },
                value,
            },
        }
    }
    fn reduction(amount: u32) -> Modifier {
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Reduction {
                selector: Selector::Category {
                    category: DamageType::Impact,
                },
                amount,
            },
        }
    }
    fn weapon(
        skill: crate::attributes::Skill,
        bonus: i32,
        wind: u64,
        descriptor: &str,
        count: u16,
    ) -> EquipmentSpec {
        let component = crate::damage::DamageComponent::rolled(
            DamageType::Energy,
            Some(DescriptorId::new(descriptor).unwrap()),
            crate::dice::DicePool::new(count, 6, 0).unwrap(),
        );
        let primary = component.key();
        EquipmentSpec {
            weapon_profile: None,
            armor: None,
            slot: EquipmentSlot::new("weapon").unwrap(),
            attack: Some(
                crate::attacks::MeleeAttack::new(
                    skill,
                    bonus,
                    wind,
                    40,
                    crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
                )
                .unwrap(),
            ),
            modifiers: vec![],
        }
    }
    #[test]
    fn ordered_strikes_no_damage_natural_source_still_has_a_gear_score() {
        let mut species = crate::test_creatures::species();
        species.melee = crate::attacks::NaturalAttack::new(
            crate::attributes::Skill::HeavyWeaponry,
            0,
            60,
            40,
            vec![crate::rules::StrikeDef {
                offset: 0,
                unlock: None,
                check: None,
                on_success: vec![],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            }],
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(
            42,
            crate::test_creatures::content_with(species.clone(), vec![]),
        );
        let actor = game
            .spawn_actor(
                Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x: 1, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(&mut game, actor, "neutral", species);
        let before = game.clone();
        let score = gear_score(&game, actor).expect("status-only attacks remain scoreable");
        assert!(score.attack.damage.is_empty());
        assert_eq!(game, before);
    }

    #[test]
    fn wide_scalar_gear_health_dominance_does_not_wrap_through_signed_stats() {
        let (game, actor, _) = fixture();
        let mut old = gear_score(&game, actor).unwrap();
        let mut new = gear_score(&game, actor).unwrap();
        old.maximum_health = i64::MAX as u64;
        new.maximum_health = u64::MAX;
        assert!(new.dominates(&old));
        assert!(!old.dominates(&new));
        old.maximum_health = u64::MAX;
        assert!(!new.dominates(&old));
    }

    #[test]
    fn upgrades_do_not_trade_away_cached_protection_for_more_defense() {
        let (mut game, actor, at) = fixture();
        for (id, modifiers) in [
            (10, vec![defense(1), reduction(2)]),
            (11, vec![defense(3), reduction(2)]),
            (12, vec![defense(3)]),
            (13, vec![defense(1), reduction(2)]),
        ] {
            add(
                &mut game,
                actor,
                at,
                id,
                EquipmentSpec {
                    weapon_profile: None,
                    armor: None,
                    slot: EquipmentSlot::new("head_armor").unwrap(),
                    attack: None,
                    modifiers,
                },
            );
        }
        game.equip_authored(actor, ItemId(10), EquipmentSlotId(3))
            .unwrap();
        assert!(game.gear_upgrade(actor, ItemId(11), EquipmentSlotId(3)));
        assert!(!game.gear_upgrade(actor, ItemId(12), EquipmentSlotId(3)));
        assert!(!game.gear_upgrade(actor, ItemId(13), EquipmentSlotId(3)));
    }
    #[test]
    fn recovery_gear_uses_derived_period_and_quiet_reset_policy() {
        let (mut game, actor, at) = fixture();
        for (id, stat, value) in [
            (10, Stat::HealthRecoveryPeriodPercent, -50),
            (11, Stat::HealthRecoveryResetPercent, -50),
        ] {
            add(
                &mut game,
                actor,
                at,
                id,
                EquipmentSpec {
                    weapon_profile: None,
                    armor: None,
                    slot: EquipmentSlot::new("head_armor").unwrap(),
                    attack: None,
                    modifiers: vec![Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add { stat, value },
                    }],
                },
            );
            assert!(game.gear_upgrade(actor, ItemId(id), EquipmentSlotId(3)));
        }
        let before = game.clone();
        assert_eq!(
            game.choose_gear(actor, &game.observe(actor).unwrap()),
            Some(Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(3)
            }),
        );
        assert_eq!(game, before);
    }
    #[test]
    fn defense_upgrades_do_not_trade_away_recovery_period_or_quiet_progress() {
        let (mut game, actor, at) = fixture();
        for (id, stat, value) in [
            (10, Stat::HealthRecoveryPeriodPercent, 50),
            (11, Stat::HealthRecoveryResetPercent, 50),
        ] {
            // A lower baseline reset policy makes worsening it observable.
            // Use authored gear for both current and candidate projections.
            if id == 10 {
                add(
                    &mut game,
                    actor,
                    at,
                    20,
                    EquipmentSpec {
                        weapon_profile: None,
                        armor: None,
                        slot: EquipmentSlot::new("body_armor").unwrap(),
                        attack: None,
                        modifiers: vec![Modifier {
                            scope: Scope::Always,
                            op: ModifierOp::Add {
                                stat: Stat::HealthRecoveryResetPercent,
                                value: -50,
                            },
                        }],
                    },
                );
                game.equip_authored(actor, ItemId(20), EquipmentSlotId(2))
                    .unwrap();
            }
            add(
                &mut game,
                actor,
                at,
                id,
                EquipmentSpec {
                    weapon_profile: None,
                    armor: None,
                    slot: EquipmentSlot::new("head_armor").unwrap(),
                    attack: None,
                    modifiers: vec![
                        defense(5),
                        Modifier {
                            scope: Scope::Always,
                            op: ModifierOp::Add { stat, value },
                        },
                    ],
                },
            );
            assert!(!game.gear_upgrade(actor, ItemId(id), EquipmentSlotId(3)));
        }
    }
    #[test]
    fn weapon_upgrades_use_owned_attributes_and_preserve_descriptor_damage_and_timing() {
        use crate::attributes::Skill;
        let (mut game, actor, at) = fixture();
        for (id, gear) in [
            (10, weapon(Skill::HeavyWeaponry, 0, 60, "fire", 1)),
            (11, weapon(Skill::LightWeaponry, 1, 60, "fire", 2)),
            (12, weapon(Skill::HeavyWeaponry, 0, 60, "fire", 2)),
            (13, weapon(Skill::HeavyWeaponry, 0, 60, "cold", 3)),
            (14, weapon(Skill::HeavyWeaponry, 1, 100, "fire", 2)),
        ] {
            add(&mut game, actor, at, id, gear);
        }
        game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
            .unwrap();
        let before = game.clone();
        assert!(!game.gear_upgrade(actor, ItemId(11), EquipmentSlotId(0)));
        assert!(game.gear_upgrade(actor, ItemId(12), EquipmentSlotId(0)));
        assert!(!game.gear_upgrade(actor, ItemId(13), EquipmentSlotId(0)));
        assert!(!game.gear_upgrade(actor, ItemId(14), EquipmentSlotId(0)));
        assert_eq!(game, before);
    }
}
