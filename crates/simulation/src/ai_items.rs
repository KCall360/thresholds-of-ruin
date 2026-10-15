//! Item decisions inspect definitions only after the acting actor knows them.
use crate::{
    Action, ActorId, EquipmentSlotId, EquipmentSpec, Game, ItemId, Observation, TravelStep,
};
use tor_world::Location;

impl Game {
    pub(crate) fn choose_nearby_loot(
        &self,
        actor: ActorId,
        view: &Observation,
        route: &mut impl FnMut(Location) -> Option<Vec<TravelStep>>,
    ) -> Option<Action> {
        let has_healing = view
            .inventory
            .iter()
            .any(|item| self.known_healing(actor, item.id));
        view.ground_items
            .iter()
            .filter(|item| {
                self.gear_destination(actor, item.id).is_some()
                    || (!has_healing && self.known_healing(actor, item.id))
            })
            .filter(|item| !self.exposed_to_visible_hostile(actor, item.location, view))
            .filter_map(|item| {
                let path = route(item.location)?;
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
                        quantity: Some(1),
                    }
                };
                Some((path.len(), item.id, action))
            })
            .min_by_key(|(distance, item, _)| (*distance, *item))
            .map(|(_, _, action)| action)
    }

    fn known_gear(&self, actor: ActorId, item: ItemId) -> Option<&EquipmentSpec> {
        let item = self.items.get(&item)?;
        let actor = self.actors.get(&actor)?;
        if item.spec.concealed && !actor.knowledge.contains(&item.spec.identity) {
            return None;
        }
        item.spec.equipment.as_ref()
    }

    pub(crate) fn exposed_to_visible_hostile(
        &self,
        actor: ActorId,
        at: Location,
        view: &Observation,
    ) -> bool {
        view.visible_actors
            .iter()
            .filter(|other| self.hostile(actor, other.id))
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
            if old.is_some_and(|old| self.known_gear(actor, old).is_none()) {
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
        let mut candidate = self.clone();
        let mut equipment = candidate.actors.get(&actor)?.equipment.clone();
        equipment.insert(slot, item);
        let sources = equipment
            .iter()
            .map(|(socket, id)| {
                let equipment = candidate.items.get(id)?.spec.equipment.as_ref()?;
                Some(crate::creatures::EquipmentSource {
                    item: *id,
                    socket: *socket,
                    modifiers: equipment.modifiers.clone(),
                    attack: equipment.attack.clone(),
                })
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
        let (Some(before), Some(after)) = (gear_score(self, actor), gear_score(&candidate, actor))
        else {
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
                    defense: DefenseKind::Physical,
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
