use super::*;
use crate::attributes::{ManaBinding, Skill};
use crate::creatures::{
    record::BuildRecord, Content, CreatureBuild, PurchaseLocation, PurchaseSelection,
    ScenarioCatalog,
};
use crate::progression::{HdLedger, HdSource};
use crate::rules::{Requirement, Ruleset, ThreatOverride, ThreatSource};
use crate::{HdCount, HdIndex};

fn expertise(id: &str) -> ExpertiseId {
    ExpertiseId::new(id).unwrap()
}
fn talent(id: &str) -> TalentId {
    TalentId::new(id).unwrap()
}
fn document() -> crate::rules::RulesetDocument {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.expertises = vec![
        ExpertiseDef {
            id: expertise("fixture.knives"),
            name: "Knives".into(),
            category: ExpertiseCategory::Equipment,
        },
        ExpertiseDef {
            id: expertise("fixture.navigation"),
            name: "Navigation".into(),
            category: ExpertiseCategory::Utility,
        },
    ];
    for id in ["fixture_knife_training", "fixture_other_training"] {
        let mut definition = crate::test_creatures::talent(id, vec![]);
        definition.expertise_grants = vec![expertise("fixture.knives")];
        document.talents.push(definition);
        document.growth.threat.overrides.push(ThreatOverride {
            source: ThreatSource::Talent { id: talent(id) },
            points: 0,
        });
    }
    document.growth.threat.overrides.push(ThreatOverride {
        source: ThreatSource::Expertise {
            id: expertise("fixture.knives"),
        },
        points: 10,
    });
    document
}
fn build_with(document: crate::rules::RulesetDocument, count: usize) -> CreatureBuild {
    let species = crate::test_creatures::species();
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
        HdLedger::seeded(vec![HdSource::Racial; count], 42).unwrap(),
        ManaBinding::Intellect,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap()
}
fn build() -> CreatureBuild {
    build_with(document(), 4)
}

#[test]
fn expertise_shares_the_exact_hd_point_budget_with_talents_and_skill_ranks() {
    let mut build = build();
    build
        .select_expertise(HdIndex(0), expertise("fixture.knives"))
        .unwrap();
    build.train(HdIndex(1), Skill::Athletics).unwrap();
    build
        .select_talent(HdIndex(2), talent("fixture_knife_training"))
        .unwrap();
    assert_eq!(build.unspent_points(), 1);
    assert_eq!(build.point_balances(HdIndex(0)).unwrap()[0].spent, 1);
    let before = build.clone();
    assert_eq!(
        build.select_expertise(HdIndex(0), expertise("fixture.navigation")),
        Err(BuildError::NoAdvancementPoint)
    );
    assert_eq!(build, before);
    assert_eq!(
        build.select_expertise(HdIndex(3), expertise("fixture.knives")),
        Err(BuildError::DuplicateExpertise)
    );
    assert_eq!(build, before);
    let refund = build
        .preview_replacement(
            PurchaseLocation {
                owner: HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    assert_eq!(refund.refunds()[0].points, 1);
    build.commit_replacement(refund).unwrap();
    assert_eq!(build.unspent_points(), 2);
    assert!(
        build
            .derive()
            .unwrap()
            .expertises
            .active(&expertise("fixture.knives")),
        "independent talent grant survives paid refund"
    );
}

#[test]
fn unknown_expertise_is_rejected_before_spending_and_on_record_restore() {
    let mut build = build();
    let before = build.clone();
    assert_eq!(
        build.select_expertise(HdIndex(0), expertise("unknown")),
        Err(BuildError::UnknownExpertise)
    );
    assert_eq!(build, before);
    build
        .select_expertise(HdIndex(0), expertise("fixture.knives"))
        .unwrap();
    let mut value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
    value["choices"][0]["purchases"][0]["selection"]["expertise"] = "unknown".into();
    let record: BuildRecord = serde_json::from_value(value).unwrap();
    assert_eq!(
        record.restore(build.content().clone()),
        Err(BuildError::UnknownExpertise)
    );
}

#[test]
fn dormant_talent_grant_remains_owned_and_blocks_paid_duplicate() {
    let mut doc = document();
    doc.talents
        .iter_mut()
        .find(|row| row.id == talent("fixture_knife_training"))
        .unwrap()
        .requirement = Requirement::TotalHd {
        minimum: HdCount(4),
    };
    let mut build = build_with(doc, 4);
    build
        .select_talent(HdIndex(0), talent("fixture_knife_training"))
        .unwrap();
    build.remove_latest().unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.expertises.owns(&expertise("fixture.knives")));
    assert!(!derived.expertises.active(&expertise("fixture.knives")));
    let before = build.clone();
    assert_eq!(
        build.select_expertise(HdIndex(1), expertise("fixture.knives")),
        Err(BuildError::DuplicateExpertise)
    );
    assert_eq!(build, before);
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    assert!(build
        .derive()
        .unwrap()
        .expertises
        .active(&expertise("fixture.knives")));
}

#[test]
fn independent_fixed_grants_deduplicate_effect_and_survive_one_source_removal() {
    let mut build = build();
    build
        .select_talent(HdIndex(0), talent("fixture_knife_training"))
        .unwrap();
    build
        .select_talent(HdIndex(1), talent("fixture_other_training"))
        .unwrap();
    let derived = build.derive().unwrap();
    assert_eq!(derived.expertises.active_ids().count(), 1);
    assert_eq!(
        derived.expertises.grants()[&expertise("fixture.knives")].len(),
        2
    );
    let replacement = build
        .preview_replacement(
            PurchaseLocation {
                owner: HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    build.commit_replacement(replacement).unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.expertises.active(&expertise("fixture.knives")));
    assert_eq!(
        derived.expertises.grants()[&expertise("fixture.knives")].len(),
        1
    );
}

#[test]
fn paid_grant_identity_does_not_depend_on_purchase_list_index() {
    let mut doc = document();
    doc.constants.racial_point_grants[0].points = 2;
    let mut build = build_with(doc, 1);
    build.train(HdIndex(0), Skill::Athletics).unwrap();
    build
        .select_expertise(HdIndex(0), expertise("fixture.knives"))
        .unwrap();
    let owned = build.derive().unwrap().expertises;
    let replacement = build
        .preview_replacement(
            PurchaseLocation {
                owner: HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    build.commit_replacement(replacement).unwrap();
    assert_eq!(build.derive().unwrap().expertises, owned);
    assert_eq!(build.choices()[0].purchases.len(), 1);
}

#[test]
fn choose_again_drain_discards_paid_expertise_instead_of_inventing_retained_ownership() {
    let mut build = build();
    build
        .select_expertise(HdIndex(3), expertise("fixture.knives"))
        .unwrap();
    let removed = build.remove_latest().unwrap();
    assert_eq!(
        removed.choices.purchases[0].selection,
        PurchaseSelection::Expertise(expertise("fixture.knives"))
    );
    assert!(!build
        .derive()
        .unwrap()
        .expertises
        .owns(&expertise("fixture.knives")));
    build.add_hit_die(HdSource::Racial, 42).unwrap();
    assert!(!build
        .derive()
        .unwrap()
        .expertises
        .owns(&expertise("fixture.knives")));
    assert_eq!(build.point_balances(HdIndex(3)).unwrap()[0].spent, 0);
}

#[test]
fn expertise_record_roundtrip_and_stale_replacement_preserve_exact_owned_sources() {
    let mut build = build();
    build
        .select_expertise(HdIndex(0), expertise("fixture.knives"))
        .unwrap();
    build
        .select_talent(HdIndex(1), talent("fixture_knife_training"))
        .unwrap();
    let bytes = serde_json::to_vec(&BuildRecord::capture(&build)).unwrap();
    let reopened = serde_json::from_slice::<BuildRecord>(&bytes)
        .unwrap()
        .restore(build.content().clone())
        .unwrap();
    assert_eq!(reopened, build);
    assert_eq!(
        reopened.derive().unwrap().expertises,
        build.derive().unwrap().expertises
    );
    let stale = build
        .preview_replacement(
            PurchaseLocation {
                owner: HdIndex(0),
                index: 0,
            },
            None,
        )
        .unwrap();
    build.train(HdIndex(2), Skill::Athletics).unwrap();
    let before = build.clone();
    assert_eq!(
        build.commit_replacement(stale),
        Err(BuildError::StaleAdvancementPreview)
    );
    assert_eq!(build, before);
}

#[test]
fn threat_prices_one_effective_combat_expertise_independent_of_grant_count() {
    let baseline = build();
    let base = crate::earned_growth::assess(&baseline, &baseline.derive().unwrap())
        .unwrap()
        .total;
    for (paid, grants) in [(true, 0), (false, 1), (false, 2), (true, 2)] {
        let mut build = build();
        if paid {
            build
                .select_expertise(HdIndex(0), expertise("fixture.knives"))
                .unwrap();
        }
        for (owner, name) in ["fixture_knife_training", "fixture_other_training"]
            .into_iter()
            .take(grants)
            .enumerate()
        {
            build
                .select_talent(HdIndex((owner + 1) as u64), talent(name))
                .unwrap();
        }
        let assessment = crate::earned_growth::assess(&build, &build.derive().unwrap()).unwrap();
        assert_eq!(assessment.total, base + 10);
        assert_eq!(
            assessment
                .sources
                .iter()
                .filter(|row| matches!(row.source, ThreatSource::Expertise { .. }))
                .count(),
            1
        );
    }
    let mut noncombat = build();
    noncombat
        .select_expertise(HdIndex(0), expertise("fixture.navigation"))
        .unwrap();
    assert_eq!(
        crate::earned_growth::assess(&noncombat, &noncombat.derive().unwrap())
            .unwrap()
            .total,
        base
    );
}

#[test]
fn catalog_rejects_unknown_duplicate_and_oversized_talent_grants() {
    for variant in 0..3 {
        let mut doc = document();
        let grant = doc
            .talents
            .iter_mut()
            .find(|row| row.id == talent("fixture_knife_training"))
            .unwrap();
        match variant {
            0 => grant.expertise_grants = vec![expertise("unknown")],
            1 => grant.expertise_grants.push(expertise("fixture.knives")),
            2 => {
                grant.expertise_grants =
                    vec![expertise("fixture.knives"); crate::limits::MODIFIERS_PER_SOURCE + 1]
            }
            _ => unreachable!(),
        }
        assert!(Ruleset::validate(doc).is_err(), "variant {variant}");
    }
}

#[test]
fn expertise_respects_pool_kind_and_rejects_duplicate_paid_record() {
    let mut doc = document();
    doc.point_pools[0]
        .allowed
        .retain(|kind| *kind != crate::rules::PurchaseKind::Expertise);
    let restricted = build_with(doc, 2);
    assert!(matches!(
        restricted.preview_purchase(
            HdIndex(0),
            PointPoolId::new("advancement").unwrap(),
            PurchaseSelection::Expertise(expertise("fixture.knives"))
        ),
        Err(BuildError::PurchaseNotAllowed)
    ));
    let mut build = build();
    build
        .select_expertise(HdIndex(0), expertise("fixture.knives"))
        .unwrap();
    let mut value = serde_json::to_value(BuildRecord::capture(&build)).unwrap();
    value["choices"][1]["purchases"] = value["choices"][0]["purchases"].clone();
    let record: BuildRecord = serde_json::from_value(value).unwrap();
    assert_eq!(
        record.restore(build.content().clone()),
        Err(BuildError::DuplicateExpertise)
    );
}

#[test]
fn expertise_codec_requires_named_current_catalog_fields() {
    for value in [
        serde_json::json!({"id":"fixture.knives","name":"Knives"}),
        serde_json::json!({"id":"fixture.knives","name":"Knives","category":"unknown"}),
        serde_json::json!({"id":"fixture.knives","name":"Knives","category":"equipment","active":true}),
        serde_json::json!(["fixture.knives", "Knives", "equipment"]),
    ] {
        assert!(serde_json::from_value::<ExpertiseDef>(value).is_err());
    }
    let mut value = serde_json::to_value(document()).unwrap();
    value.as_object_mut().unwrap().remove("expertises");
    assert!(serde_json::from_value::<crate::rules::RulesetDocument>(value).is_err());
}

#[test]
fn replacing_the_last_fixed_grant_with_paid_expertise_checks_candidate_ownership() {
    for other_grant in [false, true] {
        let mut build = build();
        build
            .select_talent(HdIndex(0), talent("fixture_knife_training"))
            .unwrap();
        if other_grant {
            build
                .select_talent(HdIndex(1), talent("fixture_other_training"))
                .unwrap();
        }
        let before = build.clone();
        let preview = build.preview_replacement(
            PurchaseLocation {
                owner: HdIndex(0),
                index: 0,
            },
            Some(crate::creatures::AdvancementPurchase {
                pool: PointPoolId::new("advancement").unwrap(),
                selection: PurchaseSelection::Expertise(expertise("fixture.knives")),
            }),
        );
        if other_grant {
            assert!(matches!(preview, Err(BuildError::DuplicateExpertise)));
            assert_eq!(build, before);
        } else {
            build.commit_replacement(preview.unwrap()).unwrap();
            assert_eq!(build.point_balances(HdIndex(0)).unwrap()[0].spent, 1);
            let derived = build.derive().unwrap();
            let origins = &derived.expertises.grants()[&expertise("fixture.knives")];
            assert_eq!(origins.len(), 1);
            assert!(matches!(
                origins.first_key_value().unwrap().0,
                ExpertiseOrigin::HdPurchase { .. }
            ));
        }
    }
}
