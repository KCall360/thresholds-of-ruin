//! Explicit simulation-to-disclosure mappings. Wire catalogs evolve separately
//! from backend records; exhaustive matches catch missing rules at compile time.
use tor_protocol as p;
use tor_simulation as s;

macro_rules! catalog_view {
    ($name:ident, $from:path, $to:path; $($variant:ident),+ $(,)?) => {
        fn $name(value: $from) -> $to {
            use $from as From;
            use $to as To;
            match value { $(From::$variant => To::$variant),+ }
        }
    };
}

catalog_view!(kind, s::progression::CreatureType, p::CreatureType;
    Aberration, Animal, Construct, Dragon, Elemental, Fey, Giant, Humanoid,
    MagicalBeast, MonstrousHumanoid, Ooze, Outsider, Plant, Undead, Vermin);
catalog_view!(subtype, s::creatures::Subtype, p::CreatureSubtype;
    Air, Angel, Aquatic, Archon, Augmented, Chaotic, Cold, Earth, Evil,
    Extraplanar, Fire, Goblinoid, Good, Incorporeal, Lawful, Native, Reptilian,
    Shapechanger, Swarm, Water);
catalog_view!(binding, s::attributes::ManaBinding, p::ManaBinding;
    Intellect, Willpower, Awareness, Presence);
catalog_view!(resource, s::resources::Resource, p::Resource; Stamina, Focus, Mana);
catalog_view!(skill, s::attributes::Skill, p::Skill;
    Athletics, HeavyWeaponry, Agility, LightWeaponry, Stealth, Thievery,
    Crafting, Deduction, Lore, Medicine, Discipline, Intimidation, Insight,
    Perception, Survival, Deception, Leadership, Persuasion, Spellcasting);
catalog_view!(talent, s::talents::Talent, p::Talent;
    Hardiness, Toughness, Unyielding, Indomitable, Endurance, DeepEndurance,
    Tireless, Guard, GreaterGuard, IronGuard, HeavyBlows, MightyBlows,
    CrushingBlows, PerfectedBlows, ImpactWard, KeenWard, EnergyWard, Resolve,
    PowerStrike, MagicBolt, Fear, ArcaneReserve, PotentBolt, EmpoweredBolt,
    GreaterBolt, MasterBolt, FearMastery);
catalog_view!(technique, s::grants::Ability, p::Technique;
    BasicMelee, PowerStrike, MagicBolt, Fear);

catalog_view!(attribute, s::attributes::Attribute, p::InspectionAttribute;
    Strength, Speed, Intellect, Willpower, Awareness, Presence);
catalog_view!(descriptor, s::grants::Descriptor, p::InspectionDescriptor; Fire, Cold, Fear, MindAffecting);
catalog_view!(category, s::combat::DamageType, p::DamageType; Energy, Impact, Keen, Spirit, Vital);

fn selector(value: s::grants::Selector) -> p::InspectionSelector {
    match value {
        s::grants::Selector::Category(value) => p::InspectionSelector::Category {
            category: category(value),
        },
        s::grants::Selector::Descriptor(value) => p::InspectionSelector::Descriptor {
            descriptor: descriptor(value),
        },
    }
}

#[path = "combat_diagnostics_view.rs"]
mod diagnostics;
pub(crate) use diagnostics::{combat_diagnostics, DiagnosticProjectionError};

fn attributes(value: s::attributes::Attributes) -> p::AttributeView {
    use s::attributes::Attribute as A;
    p::AttributeView {
        strength: value.get(A::Strength),
        speed: value.get(A::Speed),
        intellect: value.get(A::Intellect),
        willpower: value.get(A::Willpower),
        awareness: value.get(A::Awareness),
        presence: value.get(A::Presence),
    }
}

fn hd_source(value: s::progression::HdSource) -> p::HitDieSource {
    match value {
        s::progression::HdSource::Racial => p::HitDieSource::Racial,
        s::progression::HdSource::Class(s::progression::Class::Warrior) => p::HitDieSource::Warrior,
        s::progression::HdSource::Class(s::progression::Class::Mage) => p::HitDieSource::Mage,
    }
}

#[path = "creature_inspection_view.rs"]
mod inspection;
pub(crate) use inspection::{attack_view, inspect};

pub(crate) fn own_stats(stats: s::creatures::OwnStats) -> p::OwnStats {
    p::OwnStats {
        kind: kind(stats.kind),
        subtypes: stats.subtypes.into_iter().map(subtype).collect(),
        hit_dice: stats.hit_dice.into_iter().map(hd_source).collect(),
        attributes: attributes(stats.attributes),
        skills: s::attributes::Skill::ALL
            .into_iter()
            .map(|value| p::SkillView {
                skill: skill(value),
                rank: stats.skills.get(value),
            })
            .collect(),
        defenses: p::DefenseView {
            physical: stats.defenses.physical,
            cognitive: stats.defenses.cognitive,
            spiritual: stats.defenses.spiritual,
        },
        binding: binding(stats.binding),
        resources: stats
            .resources
            .into_iter()
            .map(|value| p::ResourceView {
                resource: resource(value.resource),
                balance: value.balance,
                maximum: value.maximum,
                available: value.available,
                reserved: value.reserved,
            })
            .collect(),
        active_talents: stats.active_talents.into_iter().map(talent).collect(),
        dormant_talents: stats.dormant_talents.into_iter().map(talent).collect(),
        abilities: stats.abilities.into_iter().map(technique).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, num::NonZeroU64};

    fn game() -> (s::Game, s::ActorId, s::ActorId) {
        let mut game = s::Game::two_room_in_stone(42);
        let build = s::creatures::CreatureBuild::new(
            s::creatures::Species {
                id: "inspected".into(),
                kind: s::progression::CreatureType::Humanoid,
                subtypes: BTreeSet::from([s::creatures::Subtype::Goblinoid]),
                default_attributes: s::attributes::Attributes::new([2, 1, 3, 2, 1, 0]).unwrap(),
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
                            s::dice::DicePool::new(1, 6, 0).unwrap(),
                        );
                        let primary = component.key();
                        tor_simulation::damage::DamageSpec::new(vec![component], Some(primary))
                            .unwrap()
                    },
                )
                .unwrap(),
                grants: vec![s::grants::Grant::Ability(s::grants::Ability::Fear)],
            },
            s::progression::HdLedger::seeded(
                vec![
                    s::progression::HdSource::Racial,
                    s::progression::HdSource::Class(s::progression::Class::Warrior),
                    s::progression::HdSource::Class(s::progression::Class::Mage),
                ],
                91,
            )
            .unwrap(),
            s::attributes::ManaBinding::Intellect,
        )
        .unwrap();
        let mut actors = Vec::new();
        for x in [1, 2] {
            let actor = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                s::CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: format!("team {x}"),
                },
                build.clone(),
            )
            .unwrap();
            actors.push(actor);
        }
        game.refresh_navigation();
        (game, actors[0], actors[1])
    }

    fn view(game: &s::Game, actor: s::ActorId, revision: u64) -> p::StateView {
        let (observation, scene) = game.observe_scene(actor).unwrap();
        p::StateView {
            wizard_game: false,
            revision,
            observation: crate::adapt::observation(
                observation,
                scene,
                "inspection",
                true,
                &|_| [None, None, None],
                &crate::wire_adapter::TargetScope::new(uuid::Uuid::nil(), actor),
            ),
        }
    }

    #[test]
    fn zero_hd_death_snapshot_and_delta_validate() {
        let (mut game, actor, _) = game();
        let base = view(&game, actor, 1);
        let mut build = game.creature(actor).unwrap().build().clone();
        while build.remove_latest().is_some() {}
        assert!(game.rebuild_creature(actor, build).unwrap().died);
        let next = view(&game, actor, 2);
        assert!(next.observation.combat.as_ref().unwrap().dead);
        assert!(next.validate().is_ok());
        let delta = p::StateDelta::between(&base, &next).unwrap();
        assert_eq!(delta.apply(&base).unwrap(), next);
    }

    #[test]
    fn personal_stats_survive_wire_and_delta_with_qualitative_enemy_views() {
        let (mut game, actor, target) = game();
        let base = view(&game, actor, 1);
        assert!(base.validate().is_ok());
        let combat = base.observation.combat.as_ref().unwrap();
        let stats = combat.own_stats.as_ref().unwrap();
        assert_eq!(
            stats.hit_dice,
            [
                p::HitDieSource::Racial,
                p::HitDieSource::Warrior,
                p::HitDieSource::Mage
            ]
        );
        assert_eq!(stats.attributes.intellect, 3);
        assert_eq!(stats.skills.len(), 19);
        assert_eq!(stats.resources.len(), 3);
        assert_eq!(stats.binding, p::ManaBinding::Intellect);
        assert_eq!(stats.subtypes, [p::CreatureSubtype::Goblinoid]);
        assert_eq!(combat.actors.len(), 1);
        let encoded = serde_json::to_string(&base).unwrap();
        assert_eq!(encoded.matches("own_stats").count(), 1);
        for private in ["health_seed", "reservation_owner", "grant_sources"] {
            assert!(!encoded.contains(private));
        }
        assert_eq!(
            serde_json::from_str::<p::StateView>(&encoded).unwrap(),
            base
        );

        game.admit_intention(
            actor,
            s::Action::UseAbility {
                ability: s::grants::Ability::Fear,
                target,
            },
            s::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let next = view(&game, actor, 2);
        assert!(next.validate().is_ok());
        let stats = next
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap();
        let focus = stats
            .resources
            .iter()
            .find(|r| r.resource == p::Resource::Focus)
            .unwrap();
        assert_eq!(
            (
                focus.balance,
                focus.maximum,
                focus.available,
                focus.reserved
            ),
            (3, 4, 2, 1)
        );
        let delta = p::StateDelta::between(&base, &next).unwrap();
        let delta: p::StateDelta =
            serde_json::from_str(&serde_json::to_string(&delta).unwrap()).unwrap();
        assert_eq!(delta.apply(&base).unwrap(), next);
        let enemy = view(&game, target, 1);
        assert!(enemy
            .observation
            .combat
            .unwrap()
            .own_stats
            .unwrap()
            .resources
            .iter()
            .all(|r| r.reserved == 0));

        for mutate in [0, 1, 2, 3, 4, 5, 6, 7] {
            let mut invalid = next.clone();
            let stats = invalid
                .observation
                .combat
                .as_mut()
                .unwrap()
                .own_stats
                .as_mut()
                .unwrap();
            match mutate {
                0 => stats.skills.pop().map(|_| ()).unwrap(),
                1 => stats.resources[0].available = u32::MAX,
                2 => stats.resources[0].resource = stats.resources[1].resource,
                3 => stats.subtypes.push(stats.subtypes[0]),
                4 => stats.skills[0].rank = 6,
                5 => {
                    stats.active_talents.push(p::Talent::Fear);
                    stats.dormant_talents.push(p::Talent::Fear);
                }
                6 => stats.attributes.strength = 1_001,
                7 => stats.abilities.push(stats.abilities[0]),
                _ => unreachable!(),
            }
            assert_eq!(invalid.validate(), Err(p::InvalidState::InvalidCombat));
        }
    }

    #[test]
    fn named_catalog_mappings_preserve_every_variant_without_collisions() {
        fn distinct<T: Ord>(values: impl Iterator<Item = T>, expected: usize) {
            assert_eq!(values.collect::<BTreeSet<_>>().len(), expected);
        }
        distinct(s::progression::CreatureType::ALL.into_iter().map(kind), 15);
        distinct(s::creatures::Subtype::ALL.into_iter().map(subtype), 20);
        distinct(s::attributes::Skill::ALL.into_iter().map(skill), 19);
        distinct(s::talents::Talent::ALL.into_iter().map(talent), 27);
        distinct(s::resources::Resource::ALL.into_iter().map(resource), 3);
    }
}
