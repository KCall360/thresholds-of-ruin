//! Explicit mapping of trusted creature data into independent privileged DTOs.
//! No perception work, mutation, actor loading or simulation-record serialization.
use super::{
    attribute, attributes, category, hd_source, kind, own_stats, selector, skill, subtype, talent,
    technique,
};
use tor_protocol as p;
use tor_simulation as s;

catalog_view!(class, s::progression::Class, p::InspectionClass; Warrior, Mage);

pub(crate) fn attack_view(value: &s::attacks::MeleeAttack) -> p::AttackView {
    let primary = value.damage().primary().expect("validated melee primary");
    p::AttackView {
        skill: skill(value.skill()),
        bonus: value.bonus(),
        wind_up: value.wind_up().try_into().expect("bounded melee wind-up"),
        recovery: value.recovery().try_into().expect("bounded melee recovery"),
        damage: p::DamageView {
            primary: p::DamageKeyView {
                category: category(primary.category),
                descriptor: primary.descriptor.map(super::descriptor),
                sides: primary.sides,
            },
            components: value
                .damage()
                .components()
                .iter()
                .map(|component| p::DamageComponentView {
                    category: category(component.key().category),
                    descriptor: component.key().descriptor.map(super::descriptor),
                    amount: match component.amount() {
                        s::damage::DamageAmount::Fixed(value) => {
                            p::DamageAmountView::Fixed { value }
                        }
                        s::damage::DamageAmount::Rolled(pool) => p::DamageAmountView::Rolled {
                            count: pool.count(),
                            sides: pool.sides(),
                            bonus: pool.bonus(),
                        },
                    },
                })
                .collect(),
        },
    }
}
fn grant(value: s::grants::Grant) -> p::InspectionGrant {
    use s::grants::Grant as G;
    match value {
        G::Health(amount) => p::InspectionGrant::Health { amount },
        G::Stamina(amount) => p::InspectionGrant::Stamina { amount },
        G::Focus(amount) => p::InspectionGrant::Focus { amount },
        G::Mana(amount) => p::InspectionGrant::Mana { amount },
        G::PhysicalDefense(modifier) => p::InspectionGrant::PhysicalDefense { modifier },
        G::MeleeFlat(modifier) => p::InspectionGrant::MeleeFlat { modifier },
        G::MeleeDice(count) => p::InspectionGrant::MeleeDice { count },
        G::BoltFlat(modifier) => p::InspectionGrant::BoltFlat { modifier },
        G::BoltDice(count) => p::InspectionGrant::BoltDice { count },
        G::FearDifficulty(modifier) => p::InspectionGrant::FearDifficulty { modifier },
        G::FearDuration(ticks) => p::InspectionGrant::FearDuration { ticks },
        G::Immunity(value) => p::InspectionGrant::Immunity {
            selector: selector(value),
        },
        G::Reduction(value, amount) => p::InspectionGrant::Reduction {
            selector: selector(value),
            amount,
        },
        G::Ability(value) => p::InspectionGrant::Ability {
            ability: technique(value),
        },
        G::Mindless => p::InspectionGrant::Mindless,
        G::Magical => p::InspectionGrant::Magical,
    }
}
fn source(value: &s::creatures::GrantSource) -> p::InspectionGrantSource {
    use s::creatures::GrantSource as G;
    match value {
        G::Species(id) => p::InspectionGrantSource::Species { id: id.clone() },
        G::Type(value) => p::InspectionGrantSource::Type { kind: kind(*value) },
        G::Subtype(value) => p::InspectionGrantSource::Subtype {
            subtype: subtype(*value),
        },
        G::Class(value) => p::InspectionGrantSource::Class {
            class: class(*value),
        },
        G::Template(id) => p::InspectionGrantSource::Template { id: id.clone() },
        G::Talent(value) => p::InspectionGrantSource::Talent {
            talent: talent(*value),
        },
    }
}

pub(crate) fn inspect(game: &s::Game, actor: s::ActorId) -> Option<p::CreatureInspectionView> {
    let view = game.inspect_creature(actor)?;
    let state = view.creature;
    let build = state.build();
    let species = build.species();
    let health = state.health();
    let health_contributions = build.ledger().health_contributions(state.derived().kind);
    Some(p::CreatureInspectionView {
        actor: p::ActorId(actor.0),
        tick: game.tick(),
        name: view.identity.name,
        faction: view.identity.faction,
        species: p::InspectionSpecies {
            id: species.id.clone(),
            kind: kind(species.kind),
            subtypes: species.subtypes.iter().copied().map(subtype).collect(),
            attributes: attributes(species.default_attributes),
            melee: attack_view(&species.melee),
        },
        initial_attributes: attributes(build.initial_attributes()),
        templates: build
            .templates()
            .iter()
            .map(|value| p::InspectionTemplate {
                id: value.id.clone(),
                priority: value.priority,
                kind: value.kind.map(kind),
                add_subtypes: value.add_subtypes.iter().copied().map(subtype).collect(),
                remove_subtypes: value.remove_subtypes.iter().copied().map(subtype).collect(),
                adjustments: value.adjustments,
                overrides: value
                    .overrides
                    .iter()
                    .map(|(&key, &value)| p::InspectionAttributeOverride {
                        attribute: attribute(key),
                        value,
                    })
                    .collect(),
            })
            .collect(),
        max_hp: health.maximum(),
        hp: health.current(),
        injury: health.injury(),
        dead: health.dead(),
        stats: own_stats(view.stats),
        hit_dice: build
            .ledger()
            .entries()
            .iter()
            .zip(build.choices())
            .zip(health_contributions)
            .enumerate()
            .map(
                |(index, ((die, choices), base_health))| p::InspectionHitDie {
                    ordinal: (index + 1) as u16,
                    source: hd_source(die.source()),
                    health_seed: die.health_seed(),
                    health_die: match die.source() {
                        s::progression::HdSource::Racial => state.derived().kind.health_die(),
                        s::progression::HdSource::Class(class) => class.health_die(),
                    },
                    base_health,
                    training: choices.training.iter().copied().map(skill).collect(),
                    attribute: choices.attribute.map(attribute),
                    talent: choices.talent.map(talent),
                },
            )
            .collect(),
        grants: state
            .derived()
            .grants
            .iter()
            .map(|(origin, values)| p::InspectionGrantGroup {
                source: source(origin),
                grants: values.iter().copied().map(grant).collect(),
            })
            .collect(),
        fear: state
            .fear()
            .sources()
            .iter()
            .map(|(&causer, &remaining_ticks)| p::InspectionFear {
                causer: p::ActorId(causer.0),
                remaining_ticks,
            })
            .collect(),
    })
}
