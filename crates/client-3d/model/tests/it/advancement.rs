use tor_client_3d_model::{AdvancementAction, AdvancementBrowser, AdvancementView};
use tor_client_common::advancement::AdvancementPages;
use tor_protocol::*;

fn fixture() -> (
    tor_client_common::ClientState,
    AdvancementView,
    AdvancementBalanceRow,
) {
    let state = super::projection::state();
    let context = state.reply_context();
    let basis = AdvancementBasis {
        actor: context.actor,
        branch: context.branch.clone(),
        stream: context.input.stream.stream.clone(),
        reset: context.input.stream.epoch,
        build_epoch: 3,
    };
    let row = AdvancementBalanceRow {
        owner: 1,
        pool: AdvancementContentRef {
            id: "practice".into(),
            name: "Practice".into(),
        },
        granted: 4,
        spent: 1,
        unspent: 3,
        allowed: vec![AdvancementKind::SkillRank, AdvancementKind::Talent],
    };
    let page = AdvancementPage {
        basis: basis.clone(),
        revision: context.revision,
        query: AdvancementQuery::Balances { basis, offset: 0 },
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Balance { value: row.clone() }],
    };
    page.validate().unwrap();
    (
        state,
        AdvancementView {
            context: Some(context),
            page: Some(page),
            ..Default::default()
        },
        row,
    )
}
fn catalog(view: &AdvancementView) -> AdvancementView {
    let mut result = view.clone();
    let page = result.page.as_mut().unwrap();
    page.query = AdvancementQuery::Catalog {
        basis: page.basis.clone(),
        catalog: AdvancementKind::SkillRank,
        offset: 0,
    };
    page.entries = vec![AdvancementEntry::Catalog {
        value: AdvancementCatalogRow {
            name: "Acrobatics".into(),
            selection: PurchaseChoice::SkillRank {
                skill: "acrobatics".into(),
            },
            form_choices: vec![],
        },
    }];
    page.validate().unwrap();
    result
}
#[test]
fn a_disclosed_pool_and_catalog_choice_request_a_one_based_preview_only() {
    let (state, view, row) = fixture();
    let shown = state.reply_context();
    let mut browser = AdvancementBrowser::default();
    let pool = AdvancementAction::Pool {
        row,
        kind: AdvancementKind::SkillRank,
    };
    assert_eq!(
        browser.choose(&view, &shown, &pool).as_deref(),
        Some("skills")
    );
    let catalog = catalog(&view);
    let options = browser.options(&catalog, &shown);
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].label, "Acrobatics");
    assert!(options[0].available);
    assert_eq!(
        browser
            .choose(&catalog, &shown, &options[0].action)
            .as_deref(),
        Some("purchase 2 practice skill acrobatics")
    );
    assert_eq!(
        catalog.confirm_command(&shown, true),
        None,
        "choosing never creates confirmation"
    );
}
#[test]
fn unknown_choices_wrong_pool_kinds_and_every_stale_context_are_rejected() {
    let (state, view, mut row) = fixture();
    let shown = state.reply_context();
    let mut browser = AdvancementBrowser::default();
    let catalog = catalog(&view);
    let choice = AdvancementAction::Preview(PurchaseChoice::SkillRank {
        skill: "acrobatics".into(),
    });
    assert!(
        browser.choose(&catalog, &shown, &choice).is_none(),
        "choose a pool first"
    );
    let mut forged = row.clone();
    forged.pool.id = "other".into();
    assert!(browser
        .choose(
            &view,
            &shown,
            &AdvancementAction::Pool {
                row: forged,
                kind: AdvancementKind::SkillRank
            }
        )
        .is_none());
    assert!(browser
        .choose(
            &view,
            &shown,
            &AdvancementAction::Pool {
                row: row.clone(),
                kind: AdvancementKind::Expertise
            }
        )
        .is_none());
    assert!(browser
        .choose(
            &view,
            &shown,
            &AdvancementAction::Pool {
                row: row.clone(),
                kind: AdvancementKind::SkillRank
            }
        )
        .is_some());
    assert!(browser
        .choose(
            &catalog,
            &shown,
            &AdvancementAction::Preview(PurchaseChoice::SkillRank {
                skill: "undisclosed".into()
            })
        )
        .is_none());
    for changed in 0..5 {
        let mut other = shown.clone();
        match changed {
            0 => other.actor = ActorId(99),
            1 => other.revision += 1,
            2 => other.input.stream.epoch += 1,
            3 => other.branch = BranchId("other".into()),
            _ => other.input.readiness_revision += 1,
        }
        let mut stale = browser.clone();
        assert!(stale.options(&catalog, &other).is_empty());
        assert!(stale.choose(&catalog, &other, &choice).is_none());
        stale.reconcile(&catalog, &other);
        assert!(stale.funding(&catalog, &shown).is_none());
    }
    let mut changed_basis = catalog.clone();
    changed_basis.page.as_mut().unwrap().basis.build_epoch += 1;
    assert!(browser.choose(&changed_basis, &shown, &choice).is_none());
    row.owner = u64::MAX;
    assert!(browser
        .choose(
            &view,
            &shown,
            &AdvancementAction::Pool {
                row,
                kind: AdvancementKind::SkillRank
            }
        )
        .is_none());
}
#[test]
fn talent_forms_keep_each_disclosed_acquisition_operand_and_do_not_invent_a_default() {
    let (state, view, row) = fixture();
    let shown = state.reply_context();
    let mut browser = AdvancementBrowser::default();
    browser
        .choose(
            &view,
            &shown,
            &AdvancementAction::Pool {
                row,
                kind: AdvancementKind::Talent,
            },
        )
        .unwrap();
    let mut catalog = catalog(&view);
    let page = catalog.page.as_mut().unwrap();
    page.query = AdvancementQuery::Catalog {
        basis: page.basis.clone(),
        catalog: AdvancementKind::Talent,
        offset: 0,
    };
    page.entries = vec![AdvancementEntry::Catalog {
        value: AdvancementCatalogRow {
            selection: PurchaseChoice::Talent {
                talent: "shape".into(),
            },
            name: "Shape".into(),
            form_choices: vec![
                AdvancementContentRef {
                    id: "wolf".into(),
                    name: "Wolf".into(),
                },
                AdvancementContentRef {
                    id: "bear".into(),
                    name: "Bear".into(),
                },
            ],
        },
    }];
    page.validate().unwrap();
    let options = browser.options(&catalog, &shown);
    assert_eq!(
        options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
        ["Shape — Wolf", "Shape — Bear"]
    );
    assert_eq!(
        browser
            .choose(&catalog, &shown, &options[1].action)
            .as_deref(),
        Some("purchase 2 practice talent shape form bear")
    );
    assert!(browser
        .choose(
            &catalog,
            &shown,
            &AdvancementAction::Preview(PurchaseChoice::Talent {
                talent: "shape".into()
            })
        )
        .is_none());
    assert!(browser
        .choose(
            &catalog,
            &shown,
            &AdvancementAction::Preview(PurchaseChoice::TalentForm {
                talent: "shape".into(),
                form: "dragon".into()
            })
        )
        .is_none());
}
#[test]
fn snapshots_redact_unowned_invalidated_and_unsynchronized_private_actions() {
    let (state, view, _) = fixture();
    let context = state.reply_context();
    let page = view.page.unwrap();
    let mut pages = AdvancementPages::default();
    pages
        .track(&context, "balance", context.revision, &page.query)
        .unwrap();
    pages
        .accept(&context, "balance", &page, MAX_ADVANCEMENT_PAGE_BYTES)
        .unwrap();
    let snapshot = AdvancementView::new(&pages, &state, true);
    assert_eq!(snapshot.page, Some(page));
    assert_eq!(snapshot.context, Some(context));
    let unsynchronized = AdvancementView::new(&pages, &state, false);
    assert!(unsynchronized.page.is_none() && unsynchronized.confirmation.is_none());
    pages.invalidate();
    assert!(AdvancementView::new(&pages, &state, true).page.is_none());
    let mut spectator = state.snapshot().clone();
    spectator.has_control = false;
    let observed = &mut std::sync::Arc::make_mut(&mut spectator.state).observation;
    observed.combat.as_mut().unwrap().own_stats = None;
    observed.interactions = None;
    let spectator = tor_client_common::ClientState::from_snapshot(spectator).unwrap();
    let redacted = AdvancementView::new(&pages, &spectator, true);
    assert!(redacted.page.is_none() && redacted.confirmation.is_none());
    assert_eq!(
        redacted.lines,
        ["Advancement requires control of your character."]
    );
}
#[test]
fn confirmation_requires_the_exact_summary_basis_revision_and_action_readiness() {
    let (state, mut view, _) = fixture();
    let shown = state.reply_context();
    let proposal = AdvancementProposal::Purchase {
        purchase: PurchaseOperation {
            owner: 1,
            pool: "practice".into(),
            selection: PurchaseChoice::SkillRank {
                skill: "acrobatics".into(),
            },
        },
    };
    let resource = |id: &str| AdvancementResourceRow {
        resource: AdvancementContentRef {
            id: id.into(),
            name: id.into(),
        },
        balance: 2,
        maximum: 3,
    };
    let summary = AdvancementSummary {
        strength_acquired: 0,
        proposal: proposal.clone(),
        resulting_build_epoch: 4,
        removed_count: 0,
        refund_count: 0,
        removed_condition_count: 0,
        health: AdvancementHealth {
            current: 7,
            maximum: 9,
            injury: 2,
            dead: false,
        },
        health_recovery: AdvancementHealthRecovery {
            period_ticks: 1200,
            elapsed_ticks: 0,
            stopped: false,
        },
        resources: AdvancementResources {
            stamina: resource("stamina"),
            focus: resource("focus"),
            mana: resource("mana"),
        },
        canceled_resource_holds: 0,
    };
    view.confirmation = Some(tor_client_common::advancement::Confirmation {
        basis: view.page.as_ref().unwrap().basis.clone(),
        revision: shown.revision,
        proposal,
        summary,
    });
    assert_eq!(view.confirm_command(&shown, true), Some("confirm"));
    assert_eq!(view.confirm_command(&shown, false), None);
    let mut stale = view.clone();
    stale.confirmation.as_mut().unwrap().revision += 1;
    assert_eq!(stale.confirm_command(&shown, true), None);
    let mut stale = view.clone();
    stale.confirmation.as_mut().unwrap().basis.build_epoch += 1;
    assert_eq!(stale.confirm_command(&shown, true), None);
    view.confirmation.as_mut().unwrap().summary.proposal = AdvancementProposal::GrowthSource {
        source: GrowthSource::Racial {},
    };
    assert_eq!(view.confirm_command(&shown, true), None);
}

#[test]
fn preview_balance_details_are_read_only_and_do_not_offer_hidden_pool_shortcuts() {
    let (state, mut view, row) = fixture();
    let shown = state.reply_context();
    let mut browser = AdvancementBrowser::default();
    let action = AdvancementAction::Pool {
        row,
        kind: AdvancementKind::SkillRank,
    };
    view.page.as_mut().unwrap().query = AdvancementQuery::Preview {
        basis: view.page.as_ref().unwrap().basis.clone(),
        proposal: AdvancementProposal::Purchase {
            purchase: PurchaseOperation {
                owner: 1,
                pool: "practice".into(),
                selection: PurchaseChoice::SkillRank {
                    skill: "acrobatics".into(),
                },
            },
        },
        section: AdvancementPreviewSection::Balances,
        offset: 0,
    };
    view.page.as_ref().unwrap().validate().unwrap();
    assert!(
        browser.options(&view, &shown).is_empty(),
        "preview detail rows are not selectable funding pools"
    );
    assert!(browser.choose(&view, &shown, &action).is_none());
}

#[test]
fn supply_display_mode_never_exposes_the_retained_catalog_as_hidden_shortcuts() {
    let (state, view, row) = fixture();
    let shown = state.reply_context();
    let mut browser = AdvancementBrowser::default();
    browser
        .choose(
            &view,
            &shown,
            &AdvancementAction::Pool {
                row,
                kind: AdvancementKind::SkillRank,
            },
        )
        .unwrap();
    let mut supplies = catalog(&view);
    supplies.supplies = true;
    assert!(
        browser.options(&supplies, &shown).is_empty(),
        "typed supply pages must keep their own visible content"
    );
    assert!(browser
        .choose(
            &supplies,
            &shown,
            &AdvancementAction::Preview(PurchaseChoice::SkillRank {
                skill: "acrobatics".into()
            })
        )
        .is_none());
}
