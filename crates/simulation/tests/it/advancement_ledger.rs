//! Every economy uses the same owned, bounded, reversible point ledger.
use super::creature_fixture as fixture;
use std::collections::BTreeMap;
use tor_simulation::attributes::{ManaBinding, Skill};
use tor_simulation::content_ids::{ClassId, PointPoolId, TalentId};
use tor_simulation::creatures::record::{BuildRecord, CreatureRecord};
use tor_simulation::creatures::{
    BuildAdvancement, BuildError, Content, CreatureBuild, CreatureState, PlannedAdvancement,
    PurchaseSelection, RestorationPolicy, ScenarioCatalog,
};
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::rules::{PointGrant, PointPoolDef, PurchaseKind, Ruleset, RulesetDocument};

fn pool(id: &str) -> PointPoolId {
    PointPoolId::new(id).unwrap()
}
fn talent(id: &str) -> TalentId {
    TalentId::new(id).unwrap()
}
fn subject(document: RulesetDocument, sources: Vec<HdSource>) -> CreatureBuild {
    let species = fixture::species();
    let id = species.id.clone();
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        ScenarioCatalog {
            species: BTreeMap::from([(id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    CreatureBuild::new(
        content,
        id,
        HdLedger::seeded(sources, 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::HeavyWeaponry, Skill::Spellcasting],
    )
    .unwrap()
}
fn defaults(hd: usize) -> CreatureBuild {
    subject(
        fixture::ruleset().document().clone(),
        vec![HdSource::Racial; hd],
    )
}
fn fixture_health() -> tor_simulation::rules::TalentDef {
    fixture::talent(
        "fixture_health",
        vec![fixture::add(tor_simulation::rules::Stat::Health, 2)],
    )
}
fn fixture_health_child() -> tor_simulation::rules::TalentDef {
    let mut child = fixture::talent(
        "fixture_health_child",
        vec![fixture::add(tor_simulation::rules::Stat::Health, 4)],
    );
    child.prerequisites = vec![talent("fixture_health")];
    child.requirement = tor_simulation::rules::Requirement::TotalHd {
        minimum: tor_simulation::HdCount(4),
    };
    child
}
fn fixture_defense() -> tor_simulation::rules::TalentDef {
    fixture::talent(
        "fixture_defense",
        vec![fixture::add(
            tor_simulation::rules::Stat::Defense {
                defense: tor_simulation::rules::DefenseKind::Physical,
            },
            1,
        )],
    )
}
fn authored(build: CreatureBuild) -> CreatureBuild {
    let restoration = RestorationPolicy::Authored(
        build
            .ledger()
            .entries()
            .iter()
            .zip(build.choices())
            .map(|(die, choices)| PlannedAdvancement {
                source: die.source(),
                choices: choices.clone(),
            })
            .collect(),
    );
    CreatureBuild::from_recorded(
        build.content().clone(),
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        BuildAdvancement {
            restoration,
            ..build.advancement().clone()
        },
        vec![],
    )
    .unwrap()
}

#[test]
fn default_grants_are_one_shared_point_at_every_supported_hd_count() {
    for hd in 0..=256 {
        let build = defaults(hd);
        assert_eq!(build.unspent_points(), u64::try_from(hd).unwrap());
        assert_eq!(
            build.ledger().advancement_points(build.ruleset()).unwrap(),
            u64::try_from(hd).unwrap()
        );
        assert_eq!(build.derive().unwrap().skills.get(Skill::HeavyWeaponry), 1);
        assert_eq!(build.derive().unwrap().skills.get(Skill::Spellcasting), 1);
        for owner in 0..hd {
            let balances = build
                .point_balances(tor_simulation::HdIndex::from_usize(owner).unwrap())
                .unwrap();
            assert_eq!(balances.len(), 1);
            assert_eq!(
                (
                    balances[0].granted,
                    balances[0].spent,
                    balances[0].unspent()
                ),
                (1, 0, 1)
            );
        }
    }
}
#[test]
fn separate_pools_enforce_their_kinds_and_never_borrow_each_others_balance() {
    let mut document = fixture::ruleset().document().clone();
    document.talents.extend([fixture_health()]);
    document.point_pools = vec![
        PointPoolDef {
            id: pool("skills"),
            name: "Skills".into(),
            allowed: vec![PurchaseKind::SkillRank],
        },
        PointPoolDef {
            id: pool("talents"),
            name: "Talents".into(),
            allowed: vec![PurchaseKind::Talent],
        },
    ];
    let grants = vec![
        PointGrant {
            pool: pool("skills"),
            points: 2,
        },
        PointGrant {
            pool: pool("talents"),
            points: 1,
        },
    ];
    document.constants.racial_point_grants = grants.clone();
    for class in &mut document.classes {
        class.point_grants = grants.clone();
    }
    let mut build = subject(document, vec![HdSource::Racial]);
    let before = build.clone();
    assert_eq!(
        build.purchase(
            tor_simulation::HdIndex(0),
            pool("skills"),
            PurchaseSelection::Talent(talent("fixture_health"))
        ),
        Err(BuildError::PurchaseNotAllowed)
    );
    assert_eq!(build, before);
    build
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    build
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    assert_eq!(
        build.train(tor_simulation::HdIndex(0), Skill::Athletics),
        Err(BuildError::NoAdvancementPoint)
    );
    build
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_health"))
        .unwrap();
    assert_eq!(build.unspent_points(), 0);
    assert_eq!(build.derive().unwrap().skills.get(Skill::HeavyWeaponry), 3);
    assert_eq!(
        build
            .point_balances(tor_simulation::HdIndex(0))
            .unwrap()
            .iter()
            .map(|balance| balance.spent)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    assert_eq!(
        BuildRecord::capture(&build)
            .restore(build.content().clone())
            .unwrap(),
        build
    );
}
#[test]
fn grants_reject_unknown_duplicate_empty_and_overlarge_definitions() {
    for case in 0..7 {
        let mut document = fixture::ruleset().document().clone();
        match case {
            0 => document.constants.racial_point_grants[0].pool = pool("missing"),
            1 => document
                .constants
                .racial_point_grants
                .push(document.constants.racial_point_grants[0].clone()),
            2 => document.constants.racial_point_grants[0].points = 0,
            3 => document.constants.racial_point_grants[0].points = 257,
            4 => document.point_pools[0].allowed.clear(),
            5 => document.point_pools[0].allowed.push(PurchaseKind::Talent),
            6 => document.classes[0].point_grants[0].pool = pool("missing"),
            _ => unreachable!(),
        }
        assert!(Ruleset::validate(document).is_err(), "case {case}");
    }
    let mut document = fixture::ruleset().document().clone();
    document.constants.racial_point_grants.clear();
    let mut build = subject(document, vec![HdSource::Racial]);
    assert_eq!(build.unspent_points(), 0);
    assert_eq!(
        build.train(tor_simulation::HdIndex(0), Skill::Athletics),
        Err(BuildError::NoAdvancementPoint)
    );
}
#[test]
fn grant_limits_bound_the_combined_points_and_pool_count_for_every_hd_source() {
    let mut document = fixture::ruleset().document().clone();
    for index in 0..16 {
        document.point_pools.push(PointPoolDef {
            id: pool(&format!("extra_{index}")),
            name: format!("Extra {index}"),
            allowed: vec![PurchaseKind::SkillRank, PurchaseKind::Talent],
        });
    }
    for racial in [true, false] {
        for (points, valid) in [
            (vec![128, 128], true),
            (vec![128, 129], false),
            (vec![1; 16], true),
            (vec![1; 17], false),
        ] {
            let mut candidate = document.clone();
            let grants = points
                .into_iter()
                .enumerate()
                .map(|(index, points)| PointGrant {
                    pool: candidate.point_pools[index].id.clone(),
                    points,
                })
                .collect();
            if racial {
                candidate.constants.racial_point_grants = grants;
            } else {
                candidate.classes[0].point_grants = grants;
            }
            assert_eq!(
                Ruleset::validate(candidate).is_ok(),
                valid,
                "racial={racial}"
            );
        }
    }
}

#[test]
fn starting_ranks_survive_drain_multiclassing_and_cap_purchased_ranks_at_five() {
    for skill in [Skill::HeavyWeaponry, Skill::Breath] {
        let mut build = fixture::with_starting_skills(defaults(5), [skill, Skill::Spellcasting]);
        for owner in 0..4 {
            build
                .train(tor_simulation::HdIndex::from_usize(owner).unwrap(), skill)
                .unwrap();
        }
        assert_eq!(build.derive().unwrap().skills.get(skill), 5);
        let before = build.clone();
        assert_eq!(
            build.train(tor_simulation::HdIndex(4), skill),
            Err(BuildError::RankCap)
        );
        assert_eq!(build, before);
        while build.remove_latest().is_some() {}
        assert_eq!(*build.starting_skills(), [skill, Skill::Spellcasting]);
        build
            .add_hit_die(HdSource::Class(ClassId::new("mage").unwrap()), 42)
            .unwrap();
        build
            .add_hit_die(HdSource::Class(ClassId::new("warrior").unwrap()), 42)
            .unwrap();
        assert_eq!(build.derive().unwrap().skills.get(skill), 1);
        assert_eq!(build.derive().unwrap().skills.get(Skill::Spellcasting), 1);
        assert_eq!(build.unspent_points(), 2);
    }
}

#[test]
fn breath_starting_rank_and_hd_eight_purchase_compile_to_five_and_restore_exactly() {
    use tor_simulation::attributes::{Attribute, Attributes, ByAttribute, SkillCheck};
    let mut build =
        fixture::with_starting_skills(defaults(8), [Skill::HeavyWeaponry, Skill::Breath]);
    build
        .set_initial_attributes(
            Attributes::new(ByAttribute {
                willpower: 3,
                ..ByAttribute::default()
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(build.unspent_points(), 8);
    assert_eq!(build.derive().unwrap().skills.get(Skill::Breath), 1);
    assert!(build
        .choices()
        .iter()
        .all(|choice| choice.purchases.is_empty()));
    build
        .train(tor_simulation::HdIndex(7), Skill::Breath)
        .unwrap();
    assert_eq!(build.unspent_points(), 7);
    assert_eq!(build.choices()[7].purchases.len(), 1);
    assert_eq!(
        build.choices()[7].purchases[0].selection,
        PurchaseSelection::SkillRank(Skill::Breath)
    );
    let complete = authored(build);
    let record = BuildRecord::capture(&complete);
    let encoded = serde_json::to_vec(&record).unwrap();
    let record: BuildRecord = serde_json::from_slice(&encoded).unwrap();
    let mut restored = record.restore(complete.content().clone()).unwrap();
    assert_eq!(restored, complete);
    let creature = CreatureState::new(restored.clone()).unwrap();
    let check = SkillCheck {
        skill: Skill::Breath,
        binding: creature.derived().binding(),
        modifier: 0,
        threshold: 0,
    };
    let before = tor_simulation::diagnostics::work_counts();
    for _ in 0..4 {
        let sheet = creature.derived();
        assert_eq!(sheet.attributes.get(Attribute::Willpower), 3);
        assert_eq!(sheet.skills.get(Skill::Breath), 2);
        assert_eq!(check.bonus(sheet.attributes, sheet.skills), 5);
    }
    let after = tor_simulation::diagnostics::work_counts();
    assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
    assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
    assert_eq!(
        after.advancement_choice_visits,
        before.advancement_choice_visits
    );
    assert_eq!(
        after.permanent_creature_compiles,
        before.permanent_creature_compiles
    );
    restored.remove_latest().unwrap();
    assert_eq!(restored.derive().unwrap().skills.get(Skill::Breath), 1);
    restored.restore_hit_die(42).unwrap();
    assert_eq!(restored, complete);
    restored.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(restored.derive().unwrap().skills.get(Skill::Breath), 2);
}
#[test]
fn saved_creation_rejects_duplicate_starting_skills_and_forged_purchases() {
    let build = defaults(1);
    let value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
    for case in 0..5 {
        let mut bad = value.clone();
        match case {
            0 => bad["starting_skills"] = serde_json::json!(["athletics", "athletics"]),
            1 => bad["starting_skills"] = serde_json::json!(["athletics"]),
            2 => {
                bad["choices"][0]["purchases"] = serde_json::json!([{"pool":"missing", "selection":{"kind":"skill_rank", "skill":"athletics"}}])
            }
            3 => {
                bad["choices"][0]["purchases"] = serde_json::json!([{"pool":"advancement", "selection":{"kind":"talent", "talent":"missing"}}])
            }
            4 => bad
                .as_object_mut()
                .unwrap()
                .remove("restoration")
                .map(|_| ())
                .unwrap(),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<BuildRecord>(bad).map_or(true, |record| record
                .restore(build.content().clone())
                .is_err()),
            "case {case}"
        );
    }
}
#[test]
fn saved_restoration_policy_rejects_unrecognized_fields() {
    let build = defaults(1);
    let value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
    for field in ["entries", "unexpected"] {
        let mut bad = value.clone();
        bad["restoration"][field] = serde_json::json!([]);
        assert!(
            serde_json::from_value::<BuildRecord>(bad).is_err(),
            "unknown field {field}"
        );
    }
}
#[test]
fn saved_authored_blueprints_reject_retained_source_mismatches() {
    let build = authored(defaults(2));
    let mut value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
    value["restoration"]["entries"][0]["source"] = serde_json::json!({"kind":"class", "id":"mage"});
    let record = serde_json::from_value::<BuildRecord>(value).unwrap();
    assert!(record.restore(build.content().clone()).is_err());
}
#[test]
fn saved_blueprints_validate_future_references_budgets_and_owned_choices() {
    let mut build = authored(fixture::with_talents(defaults(6), [fixture_health()]));
    for _ in 0..5 {
        build.remove_latest().unwrap();
    }
    let value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
    let rank = serde_json::json!({"pool":"advancement", "selection":{"kind":"skill_rank", "skill":"heavy_weaponry"}});
    let owned = serde_json::json!({"pool":"advancement", "selection":{"kind":"talent", "talent":"fixture_health"}});
    for case in 0..8 {
        let mut bad = value.clone();
        let entries = &mut bad["restoration"]["entries"];
        match case {
            0 => entries[5]["source"] = serde_json::json!({"kind":"class", "id":"missing"}),
            1 => {
                entries[5]["choices"]["purchases"] = serde_json::json!([{"pool":"missing", "selection":{"kind":"skill_rank", "skill":"athletics"}}])
            }
            2 => {
                entries[5]["choices"]["purchases"] = serde_json::json!([{"pool":"advancement", "selection":{"kind":"talent", "talent":"missing"}}])
            }
            3 => {
                entries[5]["choices"]["purchases"] = serde_json::json!([rank.clone(), rank.clone()])
            }
            4 => entries[5]["choices"]["attribute"] = serde_json::json!("strength"),
            5 => {
                for entry in entries.as_array_mut().unwrap() {
                    entry["choices"]["purchases"] = serde_json::json!([rank.clone()]);
                }
            }
            6 => {
                entries[0]["choices"]["purchases"] = serde_json::json!([owned.clone()]);
                entries[5]["choices"]["purchases"] = serde_json::json!([owned.clone()]);
            }
            7 => *entries = serde_json::json!([]),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_value::<BuildRecord>(bad).map_or(true, |record| record
                .restore(build.content().clone())
                .is_err()),
            "case {case}"
        );
    }
}
#[test]
fn authored_restoration_reactivates_earlier_dormant_nodes_without_replacing_retained_choices() {
    let mut build = fixture::with_talents(defaults(4), [fixture_health(), fixture_health_child()]);
    build
        .select_talent(tor_simulation::HdIndex(3), talent("fixture_health"))
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_health_child"))
        .unwrap();
    let mut build = authored(build);
    let original = build.clone();
    build.remove_latest().unwrap();
    let derived = build.derive().unwrap();
    assert!(derived
        .dormant_talents
        .contains(&talent("fixture_health_child")));
    assert!(!derived.dormant_reasons[&talent("fixture_health_child")].is_empty());
    build
        .train(tor_simulation::HdIndex(1), Skill::Athletics)
        .unwrap();
    let retained = build.choices().to_vec();
    let mut build = BuildRecord::capture(&build)
        .restore(build.content().clone())
        .unwrap();
    build.restore_hit_die(42).unwrap();
    assert_eq!(&build.choices()[..3], &retained);
    assert_eq!(build.choices()[3], original.choices()[3]);
    assert_eq!(build.ledger().entries(), original.ledger().entries());
    assert!(build
        .derive()
        .unwrap()
        .active_talents
        .contains(&talent("fixture_health_child")));
    assert_eq!(
        build.restore_hit_die(42),
        Err(BuildError::NoAuthoredRestoration)
    );
}
#[test]
fn choose_again_restoration_has_empty_points_and_preserves_injury_resources_and_death() {
    let mut build = defaults(2);
    build
        .train(tor_simulation::HdIndex(1), Skill::Athletics)
        .unwrap();
    let original = build.ledger().entries()[1].clone();
    let mut state = CreatureState::new(build.clone()).unwrap();
    state.damage(1);
    let balances = state.costs().resources().clone();
    build.remove_latest().unwrap();
    state.rebuild(build.clone()).unwrap();
    assert_eq!(
        build.restore_hit_die(42),
        Err(BuildError::NoAuthoredRestoration)
    );
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    state.rebuild(build.clone()).unwrap();
    assert_eq!(state.health().injury(), 1);
    assert_eq!(*state.costs().resources(), balances);
    assert_eq!(build.ledger().entries()[1], original);
    assert!(build.choices()[1].purchases.is_empty());
    while build.remove_latest().is_some() {}
    state.rebuild(build.clone()).unwrap();
    assert!(state.health().dead());
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    state.rebuild(build).unwrap();
    assert!(state.health().dead());
    assert_eq!(state.health().current(), 0);
    let restored = CreatureRecord::capture(&state)
        .restore(state.build().content().clone(), vec![])
        .unwrap();
    assert!(restored.health().dead());
    assert_eq!(
        restored.build().starting_skills(),
        state.build().starting_skills()
    );
}

#[test]
fn regrowing_authored_ordinals_rejects_source_changes_and_extra_ordinals_stay_empty() {
    let mut build = fixture::with_talents(defaults(1), [fixture_health()]);
    build
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_health"))
        .unwrap();
    let mut build = authored(build);
    build.remove_latest().unwrap();
    let before = build.clone();
    assert_eq!(
        build.add_hit_die(HdSource::Class(ClassId::new("mage").unwrap()), 42),
        Err(BuildError::InvalidOwnership)
    );
    assert_eq!(build, before);
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    assert_eq!(
        build.choices()[0].talents().cloned().collect::<Vec<_>>(),
        vec![talent("fixture_health")]
    );
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    build
        .train(tor_simulation::HdIndex(1), Skill::Athletics)
        .unwrap();
    build.remove_latest().unwrap();
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    assert!(build.choices()[1].purchases.is_empty());
    assert_eq!(
        *build.starting_skills(),
        [Skill::HeavyWeaponry, Skill::Spellcasting]
    );
    let RestorationPolicy::Authored(plan) = build.restoration_policy() else {
        panic!("authored provenance was lost");
    };
    assert_eq!(plan.len(), 1);
}

#[test]
fn authored_drain_and_restore_preserve_injury_condition_clocks_and_paid_reservations() {
    use tor_simulation::content_ids::ConditionId;
    use tor_simulation::costs::ResourceCost;
    use tor_simulation::resources::Resource;
    use tor_simulation::{ActorId, IntentionId};
    let mut build = fixture::with_talents(defaults(2), [fixture_health()]);
    build
        .select_talent(tor_simulation::HdIndex(1), talent("fixture_health"))
        .unwrap();
    let mut build = authored(build);
    let mut state = CreatureState::new(build.clone()).unwrap();
    state.damage(1);
    state
        .start_cost(
            IntentionId(70),
            tor_simulation::costs::CostBundle::single(ResourceCost {
                resource: Resource::Stamina,
                start: 1,
                resolution: 1,
            })
            .unwrap(),
        )
        .unwrap();
    state
        .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(9), 200)
        .unwrap();
    state.advance_active(7);
    let costs = state.costs().clone();
    let conditions = state.conditions().clone();
    build.remove_latest().unwrap();
    state.rebuild(build.clone()).unwrap();
    build.restore_hit_die(42).unwrap();
    state.rebuild(build).unwrap();
    assert_eq!(state.health().injury(), 1);
    assert_eq!(*state.costs(), costs);
    assert_eq!(*state.conditions(), conditions);
    let restored = CreatureRecord::capture(&state)
        .restore(state.build().content().clone(), vec![])
        .unwrap();
    assert_eq!(*restored.costs(), costs);
    assert_eq!(*restored.conditions(), conditions);
    assert_eq!(restored, state);
}

#[test]
fn a_shared_multi_point_grant_can_own_multiple_talents_and_a_rank_on_one_die() {
    let mut document = fixture::ruleset().document().clone();
    document
        .talents
        .extend([fixture_health(), fixture_defense()]);
    document.constants.racial_point_grants[0].points = 3;
    let mut build = subject(document, vec![HdSource::Racial]);
    build
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_health"))
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_defense"))
        .unwrap();
    assert_eq!(build.choices()[0].purchases.len(), 3);
    assert_eq!(build.unspent_points(), 0);
    assert_eq!(build.derive().unwrap().active_talents.len(), 2);
    assert_eq!(
        build.select_talent(tor_simulation::HdIndex(0), talent("fixture_defense")),
        Err(BuildError::DuplicateTalent)
    );
    let mut build = BuildRecord::capture(&build)
        .restore(build.content().clone())
        .unwrap();
    let removed = build.remove_latest().unwrap();
    assert_eq!(removed.choices.purchases.len(), 3);
    assert!(build.derive().unwrap().active_talents.is_empty());
    assert_eq!(build.derive().unwrap().skills.get(Skill::HeavyWeaponry), 1);
}

#[test]
fn a_temporarily_blocked_authored_restoration_does_not_make_a_drained_build_unsaveable() {
    use tor_simulation::attributes::{Attribute, Attributes, ByAttribute};
    let attributes = |strength| {
        Attributes::new(ByAttribute {
            strength,
            ..Default::default()
        })
        .unwrap()
    };
    let mut build = defaults(4);
    build.set_initial_attributes(attributes(4)).unwrap();
    build
        .increase_attribute(tor_simulation::HdIndex(3), Attribute::Strength)
        .unwrap();
    let mut build = authored(build);
    build.remove_latest().unwrap();
    build.set_initial_attributes(attributes(5)).unwrap();
    let restored = BuildRecord::capture(&build).restore(build.content().clone());
    assert!(
        restored.is_ok(),
        "saved provenance must not reapply future attribute choices to the current build: {restored:?}"
    );
    let mut restored = restored.unwrap();
    let before = restored.clone();
    assert_eq!(restored.restore_hit_die(42), Err(BuildError::AttributeCap));
    assert_eq!(restored, before);
    restored.set_initial_attributes(attributes(4)).unwrap();
    restored.restore_hit_die(42).unwrap();
    assert_eq!(
        restored
            .derive()
            .unwrap()
            .attributes
            .get(Attribute::Strength),
        5
    );
}

#[test]
fn purchase_previews_are_readonly_and_match_the_committed_permanent_build() {
    let mut build = fixture::with_talents(defaults(2), [fixture_health()]);
    let before = build.clone();
    let preview = build
        .preview_purchase(
            tor_simulation::HdIndex(0),
            pool("advancement"),
            PurchaseSelection::SkillRank(Skill::HeavyWeaponry),
        )
        .unwrap();
    assert_eq!(build, before);
    assert_eq!(preview.build().unspent_points(), 1);
    assert_eq!(preview.derived().skills.get(Skill::HeavyWeaponry), 2);
    assert_eq!(preview.build().ledger(), build.ledger());
    build
        .purchase(
            tor_simulation::HdIndex(0),
            pool("advancement"),
            PurchaseSelection::SkillRank(Skill::HeavyWeaponry),
        )
        .unwrap();
    assert_eq!(&build, preview.build());
    assert_eq!(&build.derive().unwrap(), preview.derived());
    let before = build.clone();
    assert_eq!(
        build.preview_purchase(
            tor_simulation::HdIndex(0),
            pool("advancement"),
            PurchaseSelection::Talent(talent("fixture_health"))
        ),
        Err(BuildError::NoAdvancementPoint)
    );
    assert_eq!(build, before);
}

#[path = "advancement_replacement.rs"]
mod replacement;

fn ownership_limit_subject() -> CreatureBuild {
    use tor_simulation::creatures::{AdvancementPurchase, HdChoices};
    use tor_simulation::rules::{Requirement, TalentDef};
    let mut document = fixture::ruleset().document().clone();
    document.constants.racial_point_grants[0].points = 256;
    document.talents = (0..257)
        .map(|index| TalentDef {
            form_choices: vec![],
            expertise_grants: vec![],
            id: talent(&format!("limit_{index:03}")),
            name: format!("Limit {index}"),
            prerequisites: vec![],
            requirement: Requirement::TotalHd {
                minimum: tor_simulation::HdCount(if index == 256 { 1 } else { 3 }),
            },
            modifiers: vec![],
        })
        .collect();
    let build = subject(document, vec![HdSource::Racial; 3]);
    let choices = vec![
        HdChoices {
            purchases: (0..256)
                .map(|index| AdvancementPurchase {
                    pool: pool("advancement"),
                    selection: PurchaseSelection::Talent(talent(&format!("limit_{index:03}"))),
                })
                .collect(),
            attribute: None,
        },
        HdChoices {
            purchases: vec![AdvancementPurchase {
                pool: pool("advancement"),
                selection: PurchaseSelection::SkillRank(Skill::Athletics),
            }],
            attribute: None,
        },
        HdChoices::default(),
    ];
    CreatureBuild::from_recorded(
        build.content().clone(),
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        BuildAdvancement {
            choices,
            ..build.advancement().clone()
        },
        vec![],
    )
    .unwrap()
}

#[test]
fn total_talent_ownership_counts_active_and_dormant_without_capping_points_or_ranks() {
    let mut build = ownership_limit_subject();
    assert_eq!(build.derive().unwrap().active_talents.len(), 256);
    assert_eq!(build.unspent_points(), 3 * 256 - 257);
    assert_eq!(build.derive().unwrap().skills.get(Skill::Athletics), 1);
    for dormant in [false, true] {
        if dormant {
            build.remove_latest().unwrap();
            assert_eq!(build.derive().unwrap().dormant_talents.len(), 256);
        }
        let before = build.clone();
        assert!(
            build
                .preview_purchase(
                    tor_simulation::HdIndex(1),
                    pool("advancement"),
                    PurchaseSelection::Talent(talent("limit_256"))
                )
                .is_err(),
            "257th talent must reject even with unspent points"
        );
        assert!(build
            .select_talent(tor_simulation::HdIndex(1), talent("limit_256"))
            .is_err());
        assert_eq!(build, before);
        let record = BuildRecord::capture(&build);
        assert_eq!(record.restore(build.content().clone()).unwrap(), build);
    }
}

#[test]
fn saved_current_and_authored_purchases_reject_the_257th_owned_talent() {
    let build = authored(ownership_limit_subject());
    let record = BuildRecord::capture(&build);
    for blueprint in [false, true] {
        let mut value = serde_json::to_value(&record).unwrap();
        let choices = if blueprint {
            &mut value["restoration"]["entries"][1]["choices"]
        } else {
            &mut value["choices"][1]
        };
        choices["purchases"].as_array_mut().unwrap().push(serde_json::json!({"pool":"advancement", "selection":{"kind":"talent", "talent":"limit_256"}}));
        let forged: BuildRecord = serde_json::from_value(value).unwrap();
        assert!(
            forged.restore(build.content().clone()).is_err(),
            "over-bound ownership in blueprint={blueprint}"
        );
    }
}
