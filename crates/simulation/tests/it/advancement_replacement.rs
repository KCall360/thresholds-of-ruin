use super::*;

fn replacement_purchase(
    selection: PurchaseSelection,
) -> tor_simulation::creatures::AdvancementPurchase {
    tor_simulation::creatures::AdvancementPurchase {
        pool: pool("advancement"),
        selection,
    }
}
fn location(owner: usize) -> tor_simulation::creatures::PurchaseLocation {
    tor_simulation::creatures::PurchaseLocation {
        owner: tor_simulation::HdIndex::from_usize(owner).unwrap(),
        index: 0,
    }
}
fn replacement_graph(minimum: u16) -> CreatureBuild {
    use tor_simulation::rules::{Requirement, TalentDef};
    let mut document = fixture::ruleset().document().clone();
    let parent = |id| Requirement::OwnedTalent { talent: talent(id) };
    let definitions = [
        (
            "root_a",
            Requirement::TotalHd {
                minimum: tor_simulation::HdCount(u64::from(minimum)),
            },
        ),
        (
            "root_b",
            Requirement::TotalHd {
                minimum: tor_simulation::HdCount(u64::from(minimum)),
            },
        ),
        (
            "branch",
            Requirement::Any {
                requirements: vec![parent("root_a"), parent("root_b")],
            },
        ),
        ("left", parent("branch")),
        ("right", parent("branch")),
        (
            "leaf",
            Requirement::All {
                requirements: vec![parent("left"), parent("right")],
            },
        ),
    ];
    for (id, requirement) in definitions {
        document.talents.push(TalentDef {
            form_choices: vec![],
            id: talent(id),
            name: id.into(),
            prerequisites: vec![],
            requirement,
            modifiers: vec![],
        });
    }
    let mut build = subject(
        document,
        vec![HdSource::Racial; usize::from(minimum.max(6))],
    );
    for (owner, id) in [
        (0, "root_a"),
        (1, "root_b"),
        (2, "branch"),
        (3, "left"),
        (4, "right"),
        (5, "leaf"),
    ] {
        build
            .select_talent(
                tor_simulation::HdIndex::from_usize(owner).unwrap(),
                talent(id),
            )
            .unwrap();
    }
    build
}

#[test]
fn replacement_preview_refunds_a_diamond_once_and_preserves_an_owned_or_alternative() {
    let mut build = authored(replacement_graph(1));
    let original = build.clone();
    let preview = build
        .preview_replacement(
            location(0),
            Some(replacement_purchase(PurchaseSelection::SkillRank(
                Skill::Athletics,
            ))),
        )
        .unwrap();
    assert_eq!(build, original);
    assert_eq!(preview.removed().len(), 1);
    assert_eq!(preview.removed()[0].location, location(0));
    assert_eq!(
        preview
            .refunds()
            .iter()
            .map(|refund| refund.points)
            .sum::<u16>(),
        1
    );
    assert_eq!(
        preview.build().unspent_points(),
        0,
        "the replacement spends the refunded root point"
    );
    assert!(preview.derived().active_talents.contains(&talent("leaf")));
    assert_eq!(preview.build().ledger(), original.ledger());
    assert_eq!(
        preview.build().starting_skills(),
        original.starting_skills()
    );
    assert_eq!(
        preview.build().restoration_policy(),
        original.restoration_policy()
    );
    assert_eq!(
        preview.build().initial_attributes(),
        original.initial_attributes()
    );
    let expected = preview.build().clone();
    let duplicate = preview.clone();
    build.commit_replacement(preview).unwrap();
    assert_eq!(build, expected);
    assert_eq!(
        build.commit_replacement(duplicate),
        Err(BuildError::StaleAdvancementPreview)
    );
    assert_eq!(build, expected);
    let preview = build.preview_replacement(location(1), None).unwrap();
    assert_eq!(
        preview.removed().len(),
        5,
        "the convergent leaf is removed once"
    );
    assert_eq!(
        preview
            .refunds()
            .iter()
            .map(|refund| refund.points)
            .sum::<u16>(),
        5
    );
    assert_eq!(preview.build().unspent_points(), 5);
    assert!(preview.derived().active_talents.is_empty());
    assert!(preview.derived().dormant_talents.is_empty());
    build.commit_replacement(preview).unwrap();
    assert_eq!(
        BuildRecord::capture(&build)
            .restore(build.content().clone())
            .unwrap(),
        build
    );
}

#[test]
fn dormant_owned_or_alternatives_preserve_the_branch_until_the_last_owned_route_is_removed() {
    let mut build = replacement_graph(8);
    build.remove_latest().unwrap();
    build.remove_latest().unwrap();
    assert_eq!(build.derive().unwrap().dormant_talents.len(), 6);
    let preview = build.preview_replacement(location(0), None).unwrap();
    assert_eq!(preview.removed().len(), 1);
    assert_eq!(preview.derived().dormant_talents.len(), 5);
    assert!(preview
        .derived()
        .dormant_talents
        .contains(&talent("branch")));
    build.commit_replacement(preview).unwrap();
    let preview = build.preview_replacement(location(1), None).unwrap();
    assert_eq!(preview.removed().len(), 5);
    assert!(preview.derived().dormant_talents.is_empty());
}

#[test]
fn an_alternative_removed_transitively_cannot_preserve_a_dependent_branch() {
    use tor_simulation::rules::Requirement;
    let original = replacement_graph(1);
    let mut document = original.ruleset().document().clone();
    document
        .talents
        .iter_mut()
        .find(|definition| definition.id == talent("root_b"))
        .unwrap()
        .requirement = Requirement::OwnedTalent {
        talent: talent("root_a"),
    };
    let mut build = subject(document, vec![HdSource::Racial; 6]);
    for (owner, id) in [
        (0, "root_a"),
        (1, "root_b"),
        (2, "branch"),
        (3, "left"),
        (4, "right"),
        (5, "leaf"),
    ] {
        build
            .select_talent(
                tor_simulation::HdIndex::from_usize(owner).unwrap(),
                talent(id),
            )
            .unwrap();
    }
    let preview = build.preview_replacement(location(0), None).unwrap();
    assert_eq!(preview.removed().len(), 6);
    assert_eq!(preview.build().unspent_points(), 6);
    assert!(preview.derived().active_talents.is_empty());
}

#[test]
fn replacement_rejects_invalid_and_stale_proposals_without_mutation() {
    let mut child = fixture::talent("fixture_missing_parent", vec![]);
    child.prerequisites = vec![talent("fixture_health")];
    let mut build = fixture::with_talents(replacement_graph(1), [fixture_health(), child]);
    let before = build.clone();
    assert_eq!(
        build.preview_replacement(location(99), None),
        Err(BuildError::InvalidOwnership)
    );
    let mut missing = replacement_purchase(PurchaseSelection::SkillRank(Skill::Athletics));
    missing.pool = pool("missing");
    assert_eq!(
        build.preview_replacement(location(0), Some(missing)),
        Err(BuildError::UnknownPointPool)
    );
    assert_eq!(
        build.preview_replacement(
            location(0),
            Some(replacement_purchase(PurchaseSelection::Talent(talent(
                "root_b"
            ))))
        ),
        Err(BuildError::DuplicateTalent)
    );
    assert_eq!(
        build.preview_replacement(
            location(0),
            Some(replacement_purchase(PurchaseSelection::Talent(talent(
                "fixture_missing_parent"
            ))))
        ),
        Err(BuildError::IneligibleTalent)
    );
    assert_eq!(
        build.preview_replacement(location(0), Some(build.choices()[0].purchases[0].clone())),
        Err(BuildError::NoAdvancementChange)
    );
    assert_eq!(build, before);
    let preview = build.preview_replacement(location(0), None).unwrap();
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    let changed = build.clone();
    assert_eq!(
        build.commit_replacement(preview),
        Err(BuildError::StaleAdvancementPreview)
    );
    assert_eq!(build, changed);
}

#[test]
fn replacing_a_rank_preserves_parent_supported_talents_as_dormant_and_keeps_unrelated_dormancy() {
    use tor_simulation::rules::{Requirement, SkillSelector, TalentDef};
    let mut document = fixture::ruleset().document().clone();
    document
        .talents
        .extend([fixture_health(), fixture_health_child()]);
    document.talents.push(TalentDef {
        form_choices: vec![],
        id: talent("rank_gated"),
        name: "Rank gated".into(),
        prerequisites: vec![],
        requirement: Requirement::SkillRank {
            skill: SkillSelector::Named {
                skill: Skill::HeavyWeaponry,
            },
            minimum: 2,
        },
        modifiers: vec![],
    });
    let mut build = subject(document, vec![HdSource::Racial; 4]);
    build
        .train(tor_simulation::HdIndex(0), Skill::HeavyWeaponry)
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(1), talent("rank_gated"))
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(3), talent("fixture_health"))
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(2), talent("fixture_health_child"))
        .unwrap();
    build.remove_latest().unwrap();
    assert!(build
        .derive()
        .unwrap()
        .dormant_talents
        .contains(&talent("fixture_health_child")));
    let preview = build.preview_replacement(location(0), None).unwrap();
    assert_eq!(preview.removed().len(), 1);
    assert_eq!(preview.derived().skills.get(Skill::HeavyWeaponry), 1);
    assert!(preview
        .derived()
        .dormant_talents
        .contains(&talent("rank_gated")));
    assert!(preview
        .derived()
        .dormant_talents
        .contains(&talent("fixture_health_child")));
    assert_eq!(
        preview
            .refunds()
            .iter()
            .map(|refund| refund.points)
            .sum::<u16>(),
        1
    );
    build.commit_replacement(preview).unwrap();
    assert_eq!(build.choices()[1].purchases.len(), 1);
    assert_eq!(build.choices()[2].purchases.len(), 1);
}

#[test]
fn replacement_commit_and_recorded_replay_preserve_injury_and_existing_attribute_choices() {
    use tor_simulation::attributes::Attribute;
    let mut build = authored(replacement_graph(1));
    build
        .increase_attribute(tor_simulation::HdIndex(3), Attribute::Strength)
        .unwrap();
    let mut state = CreatureState::new(build.clone()).unwrap();
    state.damage(1);
    let costs = state.costs().clone();
    let preview = build
        .preview_replacement(
            location(0),
            Some(replacement_purchase(PurchaseSelection::SkillRank(
                Skill::Athletics,
            ))),
        )
        .unwrap();
    build.commit_replacement(preview).unwrap();
    state.rebuild(build.clone()).unwrap();
    assert_eq!(state.health().injury(), 1);
    assert_eq!(*state.costs(), costs);
    assert_eq!(
        state.build().choices()[3].attribute,
        Some(Attribute::Strength)
    );
    assert_eq!(
        CreatureRecord::capture(&state)
            .restore(build.content().clone(), vec![])
            .unwrap(),
        state
    );
    let mut replay = BuildRecord::capture(&authored(replacement_graph(1)))
        .restore(build.content().clone())
        .unwrap();
    replay
        .increase_attribute(tor_simulation::HdIndex(3), Attribute::Strength)
        .unwrap();
    let proposal = replay
        .preview_replacement(
            location(0),
            Some(replacement_purchase(PurchaseSelection::SkillRank(
                Skill::Athletics,
            ))),
        )
        .unwrap();
    replay.commit_replacement(proposal).unwrap();
    assert_eq!(replay, build);
}

#[test]
fn replacement_refunds_each_same_hd_purchase_once_without_borrowing_other_pools() {
    use tor_simulation::creatures::PurchaseLocation;
    use tor_simulation::rules::{Requirement, TalentDef};
    let mut document = fixture::ruleset().document().clone();
    document
        .talents
        .extend([fixture_health(), fixture_defense()]);
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
            points: 1,
        },
        PointGrant {
            pool: pool("talents"),
            points: 2,
        },
    ];
    document.constants.racial_point_grants = grants.clone();
    for class in &mut document.classes {
        class.point_grants = grants.clone();
    }
    document.talents.push(TalentDef {
        form_choices: vec![],
        id: talent("child"),
        name: "Child".into(),
        prerequisites: vec![],
        requirement: Requirement::OwnedTalent {
            talent: talent("fixture_health"),
        },
        modifiers: vec![],
    });
    let mut build = subject(document, vec![HdSource::Racial]);
    build
        .select_talent(tor_simulation::HdIndex(0), talent("fixture_health"))
        .unwrap();
    build
        .select_talent(tor_simulation::HdIndex(0), talent("child"))
        .unwrap();
    build
        .train(tor_simulation::HdIndex(0), Skill::Athletics)
        .unwrap();
    let before = build.clone();
    let rank = tor_simulation::creatures::AdvancementPurchase {
        pool: pool("skills"),
        selection: PurchaseSelection::SkillRank(Skill::Lore),
    };
    assert_eq!(
        build.preview_replacement(location(0), Some(rank)),
        Err(BuildError::NoAdvancementPoint)
    );
    assert_eq!(build, before);
    let replacement = tor_simulation::creatures::AdvancementPurchase {
        pool: pool("talents"),
        selection: PurchaseSelection::Talent(talent("fixture_defense")),
    };
    let preview = build
        .preview_replacement(location(0), Some(replacement))
        .unwrap();
    assert_eq!(
        preview
            .removed()
            .iter()
            .map(|value| value.location)
            .collect::<Vec<_>>(),
        vec![
            location(0),
            PurchaseLocation {
                owner: tor_simulation::HdIndex(0),
                index: 1
            }
        ]
    );
    assert_eq!(preview.refunds().len(), 1);
    assert_eq!(preview.refunds()[0].pool, pool("talents"));
    assert_eq!(preview.refunds()[0].points, 2);
    assert_eq!(
        preview
            .build()
            .point_balances(tor_simulation::HdIndex(0))
            .unwrap()
            .iter()
            .map(|balance| balance.spent)
            .collect::<Vec<_>>(),
        vec![1, 1]
    );
    build.commit_replacement(preview).unwrap();
    assert_eq!(build.choices()[0].purchases.len(), 2);
    assert_eq!(build.derive().unwrap().skills.get(Skill::Athletics), 1);
}

#[test]
fn replacement_enforces_total_owned_talent_limit_after_refunding_the_target() {
    let mut build = ownership_limit_subject();
    let before = build.clone();
    let replacement = Some(replacement_purchase(PurchaseSelection::Talent(talent(
        "limit_256",
    ))));
    assert!(
        build
            .preview_replacement(location(1), replacement.clone())
            .is_err(),
        "rank-to-talent cannot acquire a257th talent"
    );
    assert_eq!(build, before);
    let preview = build.preview_replacement(location(0), replacement).unwrap();
    assert_eq!(preview.derived().active_talents.len(), 256);
    assert_eq!(preview.removed().len(), 1);
    build.commit_replacement(preview).unwrap();
    assert!(!build
        .derive()
        .unwrap()
        .active_talents
        .contains(&talent("limit_000")));
    assert!(build
        .derive()
        .unwrap()
        .active_talents
        .contains(&talent("limit_256")));
}

#[test]
fn non_parent_or_routes_preserve_ownership_even_when_their_gate_is_currently_unmet() {
    use tor_simulation::rules::Requirement;
    let original = replacement_graph(1);
    let mut document = original.ruleset().document().clone();
    document
        .talents
        .iter_mut()
        .find(|definition| definition.id == talent("branch"))
        .unwrap()
        .requirement = Requirement::Any {
        requirements: vec![
            Requirement::OwnedTalent {
                talent: talent("root_a"),
            },
            Requirement::TotalHd {
                minimum: tor_simulation::HdCount(10),
            },
        ],
    };
    let content = Content::new(
        Ruleset::validate(document).unwrap(),
        original.content().catalog().clone(),
    )
    .unwrap();
    let build = CreatureBuild::from_recorded(
        content,
        original.species_id().clone(),
        original.initial_attributes(),
        original.ledger().clone(),
        original.binding(),
        original.advancement().clone(),
        vec![],
    )
    .unwrap();
    let preview = build.preview_replacement(location(0), None).unwrap();
    assert_eq!(preview.removed().len(), 1);
    assert_eq!(preview.refunds()[0].points, 1);
    assert_eq!(preview.derived().active_talents.len(), 1);
    assert_eq!(preview.derived().dormant_talents.len(), 4);
    assert!(preview
        .derived()
        .dormant_talents
        .contains(&talent("branch")));
    assert!(preview.derived().dormant_talents.contains(&talent("leaf")));
}

#[test]
fn replacement_preserves_health_work_and_nutrition_while_rephasing_periods() {
    use tor_simulation::{
        health_recovery::RecoveryNutrition,
        rules::{Modifier, ModifierOp, Scope, Stat},
    };
    for period_change in [0, -25] {
        for nutrition in [
            RecoveryNutrition::Normal,
            RecoveryNutrition::Slowed { multiplier: 3 },
            RecoveryNutrition::Stopped,
        ] {
            let mut document = fixture::ruleset().document().clone();
            document.talents.extend([fixture_health()]);
            document
                .talents
                .iter_mut()
                .find(|value| value.id == talent("fixture_health"))
                .unwrap()
                .modifiers
                .push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::HealthRecoveryPeriodPercent,
                        value: period_change,
                    },
                });
            let mut build = subject(document, vec![HdSource::Racial]);
            build
                .select_talent(tor_simulation::HdIndex(0), talent("fixture_health"))
                .unwrap();
            let mut build = authored(build);
            let mut state = CreatureState::new(build.clone()).unwrap();
            state.damage(3);
            state.advance_active(400);
            state.set_recovery_nutrition(nutrition).unwrap();
            let before = state.health_recovery();
            assert!(before.elapsed() > 0);
            let costs = state.costs().clone();
            let preview = build
                .preview_replacement(
                    location(0),
                    Some(replacement_purchase(PurchaseSelection::SkillRank(
                        Skill::Athletics,
                    ))),
                )
                .unwrap();
            build.commit_replacement(preview).unwrap();
            state.rebuild(build.clone()).unwrap();
            let after = state.health_recovery();
            assert_eq!(after.nutrition(), nutrition);
            assert_eq!(
                after.elapsed(),
                (u128::from(before.elapsed()) * u128::from(after.period())
                    / u128::from(before.period())) as u64
            );
            if period_change == 0 {
                assert_eq!(after, before);
            } else {
                assert!(after.period() > before.period());
            }
            assert_eq!(state.health().injury(), 3);
            assert_eq!(state.costs(), &costs);
            assert_eq!(
                CreatureRecord::capture(&state)
                    .restore(build.content().clone(), vec![])
                    .unwrap(),
                state
            );
        }
    }
}
