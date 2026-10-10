use std::num::NonZeroU64;
use tor_simulation::abilities::{AbilityEffect, AbilityReach};
use tor_simulation::attributes::Skill;
use tor_simulation::creatures::Template;
use tor_simulation::grants::{Ability, Grant};
use tor_simulation::resources::Resource;
use tor_simulation::{ActorId, CreatureIdentity, Game, GameError};
use tor_world::{Location, Position, RegionId};

pub(super) fn game() -> (Game, ActorId) {
    let mut game = Game::two_room_in_stone(42);
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let mut build = super::creature_state::build();
    build.train(0, Skill::Intimidation).unwrap();
    let mut template = Template::new("techniques", 0);
    template.grants = vec![
        Grant::Ability(Ability::PowerStrike),
        Grant::Ability(Ability::MagicBolt),
        Grant::Ability(Ability::Fear),
        Grant::BoltDice(2),
        Grant::BoltFlat(2),
        Grant::FearDifficulty(3),
        Grant::FearDuration(100),
    ];
    build.set_templates(vec![template]).unwrap();
    game.configure_creature(
        actor,
        CreatureIdentity {
            name: "subject".into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
    (game, actor)
}

#[test]
fn ability_plans_use_current_grants_shared_dice_binding_and_timing_without_mutation() {
    let (game, actor) = game();
    let before = game.clone();
    for ability in [Ability::BasicMelee, Ability::PowerStrike] {
        let plan = game.ability_plan(actor, ability).unwrap();
        assert_eq!(plan.ability, ability);
        assert_eq!((plan.preparation, plan.recovery), (50, 34));
        assert_eq!(plan.reach, AbilityReach::Melee);
        let AbilityEffect::Melee {
            impact_bonus,
            damage,
        } = plan.effect
        else {
            panic!("wrong melee effect")
        };
        let expected_bonus = if ability == Ability::PowerStrike {
            3
        } else {
            0
        };
        assert_eq!(impact_bonus, expected_bonus);
        let mut baseline_rng = 42;
        let baseline = game
            .creature(actor)
            .unwrap()
            .derived()
            .melee_damage(0)
            .unwrap()
            .resolve(
                &mut baseline_rng,
                tor_simulation::dice::Edge::default(),
                &tor_simulation::damage::Protection::default(),
            );
        let mut rng = 42;
        let resolved = damage.resolve(
            &mut rng,
            tor_simulation::dice::Edge::default(),
            &tor_simulation::damage::Protection::default(),
        );
        assert_eq!(resolved.total, baseline.total + expected_bonus);
        assert_eq!(rng, baseline_rng);
        assert_eq!(
            damage.primary().unwrap().category,
            tor_simulation::combat::DamageType::Impact
        );
        assert_eq!(
            plan.cost
                .map(|cost| (cost.resource, cost.start, cost.resolution)),
            (ability == Ability::PowerStrike).then_some((Resource::Stamina, 1, 1))
        );
    }
    let bolt = game.ability_plan(actor, Ability::MagicBolt).unwrap();
    assert_eq!((bolt.preparation, bolt.recovery), (100, 100));
    assert_eq!(bolt.reach, AbilityReach::VisibleCells(6));
    let cost = bolt.cost.unwrap();
    assert_eq!(
        (cost.resource, cost.start, cost.resolution),
        (Resource::Mana, 1, 1)
    );
    let AbilityEffect::Bolt { damage } = bolt.effect else {
        panic!("wrong bolt effect")
    };
    assert_eq!(damage.components().len(), 1);
    assert_eq!(
        damage.primary().unwrap().category,
        tor_simulation::combat::DamageType::Energy
    );
    let resolved = damage.resolve(
        &mut 42,
        tor_simulation::dice::Edge::default(),
        &tor_simulation::damage::Protection::default(),
    );
    assert!((6..=21).contains(&resolved.total));
    let fear = game.ability_plan(actor, Ability::Fear).unwrap();
    assert_eq!((fear.preparation, fear.recovery), (100, 100));
    assert_eq!(fear.reach, AbilityReach::VisibleCells(6));
    assert_eq!(
        fear.effect,
        AbilityEffect::Fear {
            difficulty: 16,
            duration: 400
        }
    );
    let cost = fear.cost.unwrap();
    assert_eq!(
        (cost.resource, cost.start, cost.resolution),
        (Resource::Focus, 1, 1)
    );
    assert_eq!(game, before);
}

#[test]
fn natural_melee_modifiers_preserve_edges_protection_and_randomness() {
    use tor_simulation::{
        combat::DamageType,
        damage::Protection,
        dice::{DicePool, Edge},
        grants::Selector,
    };
    for flat in [-10, -3, 4] {
        let (mut game, actor) = game();
        let mut build = game.creature(actor).unwrap().build().clone();
        let mut templates = build.templates().to_vec();
        let mut modifier = Template::new("natural_modifiers", 1);
        modifier.grants = vec![Grant::MeleeDice(2), Grant::MeleeFlat(flat)];
        templates.push(modifier);
        build.set_templates(templates).unwrap();
        game.rebuild_creature(actor, build).unwrap();
        let before = game.clone();
        let protection =
            Protection::from_grants([Grant::Reduction(Selector::Category(DamageType::Impact), 2)])
                .unwrap();
        for ability in [Ability::BasicMelee, Ability::PowerStrike] {
            let AbilityEffect::Melee { damage, .. } =
                game.ability_plan(actor, ability).unwrap().effect
            else {
                panic!("wrong melee effect")
            };
            let bonus = if ability == Ability::PowerStrike {
                3
            } else {
                0
            };
            let expected = DicePool::new(3, 6, flat + bonus).unwrap();
            for edge in [
                Edge::default(),
                Edge::from_counts(1, 0),
                Edge::from_counts(3, 0),
                Edge::from_counts(0, 3),
                Edge::from_counts(3, 2),
            ] {
                for seed in 0..64 {
                    let mut expected_rng = seed;
                    let rolled = expected.roll(&mut expected_rng, edge);
                    let mut actual_rng = seed;
                    let actual = damage.resolve(&mut actual_rng, edge, &protection);
                    assert_eq!(actual.total, rolled.total.saturating_sub(2));
                    assert_eq!(actual_rng, expected_rng);
                }
            }
        }
        assert_eq!(game, before, "planning and local resolution are read-only");
    }
}

#[test]
fn power_strike_combines_signed_modifiers_before_clamping_natural_and_equipped_damage() {
    use tor_simulation::{combat::DamageType, damage::Protection, dice::Edge};
    let (mut game, actor) = game();
    let mut build = game.creature(actor).unwrap().build().clone();
    let mut templates = build.templates().to_vec();
    let mut weakness = Template::new("weakness", 1);
    weakness.grants = vec![Grant::MeleeFlat(-10)];
    templates.push(weakness);
    build.set_templates(templates).unwrap();
    game.rebuild_creature(actor, build).unwrap();

    // Natural 1d6 - 10 + 3 stays zero: the bonus must precede the floor.
    for ability in [Ability::BasicMelee, Ability::PowerStrike] {
        let AbilityEffect::Melee { damage, .. } = game.ability_plan(actor, ability).unwrap().effect
        else {
            panic!("wrong melee effect")
        };
        for mut seed in 0..32 {
            assert_eq!(
                damage
                    .resolve(&mut seed, Edge::default(), &Protection::default())
                    .total,
                0
            );
        }
    }
    for (category, amount, basic_total, power_total) in [
        (DamageType::Impact, 5, 0, 0),
        (DamageType::Impact, 12, 2, 5),
        (DamageType::Keen, 12, 2, 5),
    ] {
        let mut game = game.clone();
        let mut weapon = tor_simulation::ItemSpec::ordinary("test weapon".into());
        weapon.class = tor_simulation::ItemClass::Weapon;
        weapon.equipment = Some(tor_simulation::EquipmentSpec {
            slot: tor_simulation::EquipmentSlot::Weapon,
            attack: Some(
                tor_simulation::attacks::MeleeAttack::fixed(
                    tor_simulation::attributes::Skill::HeavyWeaponry,
                    0,
                    60,
                    40,
                    category,
                    None,
                    amount,
                )
                .unwrap(),
            ),
            defense: 0,
            reductions: std::collections::BTreeMap::new(),
        });
        let item = game
            .place_item_stack(
                100,
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                Some(actor),
                1,
                weapon,
            )
            .unwrap();
        game.equip_authored(actor, item, tor_simulation::EquipmentSlotId(0))
            .unwrap();
        for (ability, expected) in [
            (Ability::BasicMelee, basic_total),
            (Ability::PowerStrike, power_total),
        ] {
            let AbilityEffect::Melee { damage, .. } =
                game.ability_plan(actor, ability).unwrap().effect
            else {
                panic!("wrong melee effect")
            };
            let mut rng = 42;
            assert_eq!(
                damage
                    .resolve(&mut rng, Edge::default(), &Protection::default())
                    .total,
                expected
            );
            assert_eq!(rng, 42, "fixed damage consumes no randomness");
            assert_eq!(damage.primary().unwrap().category, category);
            if category == DamageType::Keen && ability == Ability::PowerStrike {
                assert_eq!(damage.components().len(), 2);
                let protection = Protection::from_grants([Grant::Immunity(
                    tor_simulation::grants::Selector::Category(DamageType::Impact),
                )])
                .unwrap();
                assert_eq!(
                    damage.resolve(&mut rng, Edge::default(), &protection).total,
                    basic_total
                );
            }
        }
    }
}

#[test]
fn ability_plans_reject_unknown_dead_and_ungranted_techniques() {
    let (mut game, actor) = game();
    assert!(game.ability_plan(ActorId(999), Ability::MagicBolt).is_err());
    let mut build = game.creature(actor).unwrap().build().clone();
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert!(game.ability_plan(actor, Ability::BasicMelee).is_ok());
    for ability in [Ability::PowerStrike, Ability::MagicBolt, Ability::Fear] {
        assert_eq!(
            game.ability_plan(actor, ability),
            Err(GameError::InvalidLocation)
        );
    }
    game.apply_effects(
        actor,
        &[tor_simulation::EffectSpec::Damage {
            components: std::collections::BTreeMap::from([(
                tor_simulation::combat::DamageType::Vital,
                1_000_000,
            )]),
        }],
    )
    .unwrap();
    assert!(game.ability_plan(actor, Ability::BasicMelee).is_err());
}

#[test]
fn bolt_resolution_uses_spellcasting_and_the_shared_edge_damage_bundle() {
    use tor_simulation::{
        abilities::resolve_magic_bolt,
        attributes::SkillCheck,
        damage::{AttackCheck, Protection},
        dice::Edge,
    };
    let (game, actor) = game();
    let caster = game.creature(actor).unwrap();
    let AbilityEffect::Bolt { damage } =
        game.ability_plan(actor, Ability::MagicBolt).unwrap().effect
    else {
        panic!("wrong bolt effect")
    };
    let mut hits = 0;
    let mut misses = 0;
    for seed in 0..32 {
        for edge in [
            Edge::default(),
            Edge::from_counts(2, 0),
            Edge::from_counts(0, 2),
        ] {
            let mut expected_rng = seed;
            let expected = AttackCheck {
                check: SkillCheck {
                    skill: Skill::Spellcasting,
                    binding: caster.build().binding(),
                    modifier: 0,
                    threshold: 14,
                },
                attributes: caster.derived().attributes,
                skills: caster.derived().skills,
            }
            .resolve(&mut expected_rng, edge, &damage, &Protection::default());
            let mut actual_rng = seed;
            let actual =
                resolve_magic_bolt(caster, 14, &Protection::default(), &mut actual_rng, edge)
                    .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(actual_rng, expected_rng);
            if actual.check.success {
                hits += 1;
            } else {
                misses += 1;
            }
        }
    }
    assert!(hits > 0 && misses > 0);
}

#[test]
fn fear_resistance_uses_discipline_and_immunity_skips_randomness() {
    use tor_simulation::{
        abilities::{resolve_fear, FearResolution},
        attributes::SkillCheck,
        damage::Protection,
        dice::Edge,
        grants::{Descriptor, Selector},
    };
    let (game, actor) = game();
    let caster = game.creature(actor).unwrap();
    let defender =
        tor_simulation::creatures::CreatureState::new(super::creature_state::build()).unwrap();
    let mut resisted = 0;
    let mut applied = 0;
    for seed in 0..32 {
        let edge = Edge::from_counts(0, 2);
        let mut expected_rng = seed;
        let expected = SkillCheck {
            skill: Skill::Discipline,
            binding: defender.build().binding(),
            modifier: 0,
            threshold: 16,
        }
        .resolve(
            &mut expected_rng,
            defender.derived().attributes,
            defender.derived().skills,
            edge,
        );
        let mut actual_rng = seed;
        let actual = resolve_fear(
            caster,
            &defender,
            &Protection::default(),
            &mut actual_rng,
            edge,
        )
        .unwrap();
        assert_eq!(actual_rng, expected_rng);
        if expected.success {
            assert_eq!(actual, FearResolution::Resisted(expected));
            resisted += 1;
        } else {
            assert_eq!(
                actual,
                FearResolution::Applied {
                    resistance: expected,
                    duration: 400
                }
            );
            applied += 1;
        }
    }
    assert!(resisted > 0 && applied > 0);
    for descriptor in [Descriptor::Fear, Descriptor::MindAffecting] {
        let protection =
            Protection::from_grants([Grant::Immunity(Selector::Descriptor(descriptor))]).unwrap();
        let mut rng = 42;
        assert_eq!(
            resolve_fear(caster, &defender, &protection, &mut rng, Edge::default()).unwrap(),
            FearResolution::Immune
        );
        assert_eq!(rng, 42);
    }
}

#[test]
fn fear_duration_limit_includes_the_base_duration_and_rejects_atomically() {
    let (mut game, actor) = game();
    let mut build = game.creature(actor).unwrap().build().clone();
    let before = build.clone();
    let mut template = Template::new("long_fear", 0);
    template.grants = vec![Grant::Ability(Ability::Fear), Grant::FearDuration(999_701)];
    assert_eq!(
        build.set_templates(vec![template.clone()]),
        Err(tor_simulation::creatures::BuildError::DerivedLimit)
    );
    assert_eq!(build, before);
    template.grants[1] = Grant::FearDuration(999_700);
    build.set_templates(vec![template]).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    let AbilityEffect::Fear { duration, .. } =
        game.ability_plan(actor, Ability::Fear).unwrap().effect
    else {
        panic!("wrong fear effect")
    };
    assert_eq!(duration, 1_000_000);
}

#[test]
fn bolt_uses_the_permanent_binding_and_unavailable_resolutions_do_not_roll() {
    use tor_simulation::{
        abilities::{resolve_fear, resolve_magic_bolt},
        attributes::ManaBinding,
        creatures::{CreatureBuild, CreatureState},
        damage::Protection,
        dice::Edge,
    };
    let (game, actor) = game();
    let original = game.creature(actor).unwrap();
    let base = original.build();
    let mut build = CreatureBuild::new(
        base.species().clone(),
        base.ledger().clone(),
        ManaBinding::Presence,
    )
    .unwrap();
    build.set_templates(base.templates().to_vec()).unwrap();
    let mut caster = CreatureState::new(build.clone()).unwrap();
    let mut rng = 42;
    let result = resolve_magic_bolt(
        &caster,
        10,
        &Protection::default(),
        &mut rng,
        Edge::default(),
    )
    .unwrap();
    assert_eq!(result.check.total, i64::from(result.check.roll.kept) + 1);
    let mut dead = caster.clone();
    dead.kill();
    let mut rng = 42;
    assert!(
        resolve_magic_bolt(&dead, 10, &Protection::default(), &mut rng, Edge::default()).is_err()
    );
    assert!(resolve_fear(
        &dead,
        original,
        &Protection::default(),
        &mut rng,
        Edge::default()
    )
    .is_err());
    assert!(resolve_fear(
        original,
        &dead,
        &Protection::default(),
        &mut rng,
        Edge::default()
    )
    .is_err());
    assert_eq!(rng, 42);
    build.set_templates(vec![]).unwrap();
    caster.rebuild(build).unwrap();
    for dead in [false, true] {
        if dead {
            caster.kill();
        }
        let mut rng = 42;
        assert!(resolve_magic_bolt(
            &caster,
            10,
            &Protection::default(),
            &mut rng,
            Edge::default()
        )
        .is_err());
        assert!(resolve_fear(
            &caster,
            original,
            &Protection::default(),
            &mut rng,
            Edge::default()
        )
        .is_err());
        assert_eq!(rng, 42);
    }
}

#[derive(Default)]
struct AbilityDiagnostics {
    checks: Vec<tor_simulation::resolution_diagnostics::CheckDiagnostic>,
    immunity: Vec<(bool, bool)>,
    finished: Vec<(bool, u64)>,
    starts: Vec<(i32, u64, i64)>,
    parameters: Vec<(u16, u8, i32, u64)>,
    components: usize,
}
impl tor_simulation::resolution_diagnostics::ResolutionObserver for AbilityDiagnostics {
    fn record(&mut self, step: tor_simulation::resolution_diagnostics::ResolutionStep<'_>) {
        use tor_simulation::resolution_diagnostics::ResolutionStep as S;
        match step {
            S::Check(value) => self.checks.push(value),
            S::FearStarted {
                difficulty,
                duration,
                net_edge,
                difficulty_attribute,
                difficulty_rank,
                difficulty_bonus,
                duration_bonus,
            } => {
                self.starts.push((difficulty, duration, net_edge));
                self.parameters.push((
                    difficulty_attribute,
                    difficulty_rank,
                    difficulty_bonus,
                    duration_bonus,
                ));
            }
            S::FearImmunity {
                fear,
                mind_affecting,
            } => self.immunity.push((fear, mind_affecting)),
            S::FearFinished { applied, duration } => self.finished.push((applied, duration)),
            S::Component(_) => self.components += 1,
            _ => {}
        }
    }
}

#[test]
fn fear_diagnostics_distinguish_immunity_resistance_and_application_without_extra_rng() {
    use tor_simulation::{
        abilities::{resolve_fear, resolve_fear_with_diagnostics, FearResolution},
        damage::Protection,
        dice::Edge,
        grants::{Descriptor, Selector},
    };
    let (game, actor) = game();
    let caster = game.creature(actor).unwrap();
    let defender =
        tor_simulation::creatures::CreatureState::new(super::creature_state::build()).unwrap();
    let mut saw_resistance = false;
    let mut saw_application = false;
    for seed in 0..64 {
        let edge = Edge::from_counts(0, 3);
        let mut ordinary_rng = seed;
        let expected = resolve_fear(
            caster,
            &defender,
            &Protection::default(),
            &mut ordinary_rng,
            edge,
        )
        .unwrap();
        let mut rng = seed;
        let mut detail = AbilityDiagnostics::default();
        let actual = resolve_fear_with_diagnostics(
            caster,
            &defender,
            &Protection::default(),
            &mut rng,
            edge,
            &mut detail,
        )
        .unwrap();
        assert_eq!(actual, expected);
        assert_eq!(rng, ordinary_rng);
        assert_eq!(detail.starts, [(16, 400, -3)]);
        assert_eq!(detail.parameters, [(2, 1, 3, 100)]);
        assert_eq!(detail.checks.len(), 1);
        assert_eq!(detail.checks[0].check.skill, Skill::Discipline);
        assert_eq!(
            (
                detail.checks[0].input_edge,
                detail.checks[0].edge,
                detail.checks[0].unused_edge
            ),
            (-3, -1, -2)
        );
        assert!(detail.immunity.is_empty());
        assert_eq!(detail.components, 0);
        match actual {
            FearResolution::Resisted(check) => {
                saw_resistance = true;
                assert_eq!(detail.checks[0].outcome, check);
                assert_eq!(detail.finished, [(false, 0)]);
            }
            FearResolution::Applied {
                resistance,
                duration,
            } => {
                saw_application = true;
                assert_eq!(detail.checks[0].outcome, resistance);
                assert_eq!(detail.finished, [(true, duration)]);
            }
            FearResolution::Immune => panic!("unprotected defender"),
        }
    }
    assert!(saw_resistance && saw_application);
    for descriptor in [Descriptor::Fear, Descriptor::MindAffecting] {
        let protection =
            Protection::from_grants([Grant::Immunity(Selector::Descriptor(descriptor))]).unwrap();
        let mut rng = 42;
        let mut detail = AbilityDiagnostics::default();
        assert_eq!(
            resolve_fear_with_diagnostics(
                caster,
                &defender,
                &protection,
                &mut rng,
                Edge::default(),
                &mut detail
            )
            .unwrap(),
            FearResolution::Immune
        );
        assert_eq!(rng, 42);
        assert!(detail.checks.is_empty());
        assert_eq!(
            detail.immunity,
            [(
                descriptor == Descriptor::Fear,
                descriptor == Descriptor::MindAffecting
            )]
        );
        assert_eq!(detail.finished, [(false, 0)]);
    }
}

#[test]
fn bolt_diagnostics_use_the_real_spellcasting_damage_bundle() {
    use tor_simulation::{
        abilities::{resolve_magic_bolt, resolve_magic_bolt_with_diagnostics},
        damage::Protection,
        dice::Edge,
    };
    let (game, actor) = game();
    let caster = game.creature(actor).unwrap();
    let mut ordinary_rng = 42;
    let expected = resolve_magic_bolt(
        caster,
        0,
        &Protection::default(),
        &mut ordinary_rng,
        Edge::from_counts(2, 0),
    )
    .unwrap();
    let mut rng = 42;
    let mut detail = AbilityDiagnostics::default();
    let actual = resolve_magic_bolt_with_diagnostics(
        caster,
        0,
        &Protection::default(),
        &mut rng,
        Edge::from_counts(2, 0),
        &mut detail,
    )
    .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(rng, ordinary_rng);
    assert_eq!(detail.checks.len(), 1);
    assert_eq!(detail.checks[0].check.skill, Skill::Spellcasting);
    assert_eq!(
        detail.checks[0].attribute,
        tor_simulation::attributes::Attribute::Intellect
    );
    assert_eq!(detail.components, actual.damage.unwrap().components.len());
    assert!(detail.starts.is_empty());
    assert!(detail.finished.is_empty());
}

#[test]
fn fixed_natural_primary_combines_permanent_penalties_and_power_before_clamping() {
    use tor_simulation::{
        attacks::MeleeAttack,
        combat::DamageType,
        creatures::CreatureBuild,
        damage::{DamageComponent, DamageSpec, Protection},
        dice::Edge,
    };
    for (amount, basic, power) in [(5, 0, 0), (12, 2, 5)] {
        let (mut game, actor) = game();
        let original = game.creature(actor).unwrap().build();
        let mut species = original.species().clone();
        let component = DamageComponent::fixed(DamageType::Impact, None, amount);
        let primary = component.key();
        species.melee = MeleeAttack::new(
            Skill::HeavyWeaponry,
            0,
            60,
            40,
            DamageSpec::new(vec![component], Some(primary)).unwrap(),
        )
        .unwrap();
        let mut templates = original.templates().to_vec();
        let mut penalty = Template::new("fixed_penalty", 1);
        penalty.grants = vec![Grant::MeleeFlat(-10), Grant::MeleeDice(2)];
        templates.push(penalty);
        let build = CreatureBuild::from_recorded(
            species,
            original.initial_attributes(),
            original.ledger().clone(),
            original.binding(),
            original.choices().to_vec(),
            templates,
        )
        .unwrap();
        game.rebuild_creature(actor, build).unwrap();
        for (ability, expected) in [(Ability::BasicMelee, basic), (Ability::PowerStrike, power)] {
            let AbilityEffect::Melee { damage, .. } =
                game.ability_plan(actor, ability).unwrap().effect
            else {
                panic!("expected melee");
            };
            for edge in [
                Edge::default(),
                Edge::from_counts(3, 0),
                Edge::from_counts(0, 3),
            ] {
                let mut rng = 42;
                assert_eq!(
                    damage.resolve(&mut rng, edge, &Protection::default()).total,
                    expected
                );
                assert_eq!(
                    rng, 42,
                    "fixed source amounts have no dice to extend or reroll"
                );
            }
        }
    }
}
