use super::*;

fn context() -> ReplyContext {
    ReplyContext {
        input: InputContext {
            stream: StreamContext {
                stream: StreamId("owner-stream".into()),
                epoch: 2,
            },
            readiness_revision: 3,
        },
        actor: ActorId(1),
        branch: BranchId("main".into()),
        cursor: StreamCursor {
            sequence: 4,
            tick: 5,
        },
        revision: 6,
    }
}
fn basis() -> AdvancementBasis {
    AdvancementBasis {
        actor: ActorId(1),
        branch: BranchId("main".into()),
        stream: StreamId("owner-stream".into()),
        reset: 2,
        build_epoch: 7,
    }
}
fn content(id: &str) -> AdvancementContentRef {
    AdvancementContentRef {
        id: id.into(),
        name: id.into(),
    }
}
fn overview() -> AdvancementOverview {
    AdvancementOverview {
        growth: AdvancementGrowth {
            highest_applied_tier: 2,
            restorable_hit_dice: 0,
            restoration_blocker: None,
            highest_earned_tier: 2,
            peak_threat: 2,
            held_xp: 0,
            earned_xp: 0,
            spent_xp: 0,
            next_threshold: 100,
            future: FutureGrowth::Disabled {},
        },
        starting_skills: [content("acrobatics"), content("athletics")],
        points: AdvancementPoints {
            earned: 9,
            spent: 2,
            unspent: 7,
        },
        timings: AdvancementTimings {
            purchase_recovery: 2,
            respec_prepare: 10,
            respec_recovery: 3,
            received_loss_interval: 20,
            provisional: true,
        },
        ration: AdvancementRation {
            supply: content("ration"),
            quantity: 1,
            access: SupplyAccess::CarriedOrGroundTouch,
        },
        received_loss_remaining: 0,
        perceived_threat: false,
        ready: true,
        other_preparation: false,
        respec: None,
    }
}
fn overview_page() -> AdvancementPage {
    AdvancementPage {
        basis: basis(),
        revision: 6,
        query: AdvancementQuery::Overview {},
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Overview {
            value: Box::new(overview()),
        }],
    }
}
fn install(pages: &mut AdvancementPages, page: AdvancementPage, id: &str) -> Result<(), String> {
    pages.track(&context(), id, 6, &page.query)?;
    pages.accept(&context(), id, &page, MAX_ADVANCEMENT_PAGE_BYTES)
}
fn opened() -> AdvancementPages {
    let mut pages = AdvancementPages::default();
    install(&mut pages, overview_page(), "open").unwrap();
    pages
}
fn proposal() -> AdvancementProposal {
    AdvancementProposal::Replacement {
        replacement: ReplacementOperation {
            source: ReplacementSource { owner: 0, index: 1 },
            replacement: None,
        },
    }
}
fn summary() -> AdvancementSummary {
    let resource = |id| AdvancementResourceRow {
        resource: content(id),
        balance: 0,
        maximum: 1,
    };
    AdvancementSummary {
        proposal: proposal(),
        resulting_build_epoch: 8,
        removed_count: 33,
        refund_count: 2,
        removed_condition_count: 1,
        health: AdvancementHealth {
            current: 0,
            maximum: 2,
            injury: 3,
            dead: true,
        },
        health_recovery: AdvancementHealthRecovery {
            period_ticks: 10,
            elapsed_ticks: 0,
            stopped: true,
        },
        resources: AdvancementResources {
            stamina: resource("stamina"),
            focus: resource("focus"),
            mana: resource("mana"),
        },
        canceled_resource_holds: 1,
    }
}
fn preview_page() -> AdvancementPage {
    AdvancementPage {
        basis: basis(),
        revision: 6,
        query: AdvancementQuery::Preview {
            basis: basis(),
            proposal: proposal(),
            section: AdvancementPreviewSection::Summary,
            offset: 0,
        },
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Summary { value: summary() }],
    }
}

#[test]
fn only_the_exact_requested_owner_page_is_installed_atomically() {
    let mut pages = opened();
    let original = pages.clone();
    let page = preview_page();
    assert!(pages
        .accept(&context(), "unsolicited", &page, MAX_ADVANCEMENT_PAGE_BYTES)
        .is_err());
    assert_eq!(pages, original);
    pages.track(&context(), "preview", 6, &page.query).unwrap();
    let before = pages.clone();
    for changed in 0..5 {
        let mut wrong = page.clone();
        match changed {
            0 => wrong.basis.actor = ActorId(2),
            1 => wrong.basis.build_epoch += 1,
            2 => wrong.revision += 1,
            3 => wrong.query = AdvancementQuery::Overview {},
            _ => wrong.next = Some(1),
        }
        assert!(pages
            .accept(&context(), "preview", &wrong, MAX_ADVANCEMENT_PAGE_BYTES)
            .is_err());
        assert_eq!(pages, before);
    }
    pages
        .accept(&context(), "preview", &page, MAX_ADVANCEMENT_PAGE_BYTES)
        .unwrap();
    assert_eq!(pages.confirmation().unwrap().proposal, proposal());
}

#[test]
fn confirmation_preserves_refund_only_operands_and_never_refreshes_a_stale_preview() {
    let mut pages = opened();
    install(&mut pages, preview_page(), "preview").unwrap();
    let view = crate::test_support::observation();
    assert_eq!(
        pages
            .confirm(&context(), &view, SupplySelection::Carried)
            .unwrap(),
        Command::Act {
            expected_revision: 6,
            action: Action::Respec {
                basis: Box::new(basis()),
                replacement: match proposal() {
                    AdvancementProposal::Replacement { replacement } => replacement,
                    _ => unreachable!(),
                },
                supplies: SupplySelection::Carried
            },
        }
    );
    let mut changed = context();
    changed.revision += 1;
    pages.observe(&changed, true);
    assert!(pages
        .confirm(&changed, &view, SupplySelection::Carried)
        .is_err());
    assert!(pages.lines().iter().any(|line| line.contains("stale")));
    assert_eq!(pages.confirmation().unwrap().basis, basis());
}

#[test]
fn loss_of_control_reset_branch_or_actor_discards_all_private_state() {
    for change in 0..5 {
        let mut pages = opened();
        install(&mut pages, preview_page(), "preview").unwrap();
        let mut changed = context();
        match change {
            0 => changed.actor = ActorId(2),
            1 => changed.branch = BranchId("other".into()),
            2 => changed.input.stream.epoch += 1,
            3 => changed.input.stream.stream = StreamId("new".into()),
            _ => {}
        }
        pages.observe(&changed, change != 4);
        assert!(pages.page().is_none());
        assert!(pages.confirmation().is_none());
        assert!(pages.overview().is_none());
    }
}

#[test]
fn all_removal_pages_are_traversable_with_four_retained_pages_and_exact_query_echoes() {
    let mut pages = opened();
    let mut preview = preview_page();
    let AdvancementEntry::Summary { value } = &mut preview.entries[0] else {
        unreachable!()
    };
    value.removed_count = 100;
    install(&mut pages, preview, "preview").unwrap();
    for offset in 0..100_u16 {
        let page = AdvancementPage {
            basis: basis(),
            revision: 6,
            query: AdvancementQuery::Preview {
                basis: basis(),
                proposal: proposal(),
                section: AdvancementPreviewSection::Removed,
                offset,
            },
            total: 100,
            next: (offset < 99).then_some(offset + 1),
            entries: vec![AdvancementEntry::Removed {
                value: AdvancementLedgerRow {
                    source: ReplacementSource {
                        owner: 0,
                        index: offset,
                    },
                    pool: content("general"),
                    selection: PurchaseChoice::Talent {
                        talent: format!("talent_{offset}"),
                    },
                    name: format!("Talent {offset}"),
                    status: AdvancementStatus::Dormant,
                },
            }],
        };
        install(&mut pages, page, "next").unwrap();
        assert!(pages.retained_pages() <= 4);
        assert!(pages
            .lines()
            .iter()
            .any(|line| line.contains(&format!("Talent {offset}"))));
        if offset < 99 {
            let Command::Advancement {
                expected_revision,
                query,
            } = pages.next(&context()).unwrap()
            else {
                panic!("query")
            };
            assert_eq!(expected_revision, 6);
            assert!(
                matches!(query, AdvancementQuery::Preview { basis: b, proposal: p, section: AdvancementPreviewSection::Removed, offset: o } if b == basis() && p == proposal() && o == offset + 1)
            );
        }
    }
    assert!(pages.next(&context()).is_err());
    let Command::Advancement {
        query: AdvancementQuery::Preview { offset, .. },
        ..
    } = pages.previous(&context()).unwrap()
    else {
        panic!("previous")
    };
    assert!(offset < 99);
}

#[test]
fn a_page_rejects_wrong_sections_nonprogressing_continuations_and_complete_escaped_overflow() {
    let mut pages = opened();
    let page = preview_page();
    pages.track(&context(), "preview", 6, &page.query).unwrap();
    let before = pages.clone();
    let mut wrong = page.clone();
    wrong.entries = overview_page().entries;
    assert!(pages
        .accept(&context(), "preview", &wrong, MAX_ADVANCEMENT_PAGE_BYTES)
        .is_err());
    assert_eq!(pages, before);
    let envelope = ServerMessage::AdvancementPage {
        context: context(),
        request_id: "preview".into(),
        page: Box::new(page.clone()),
    };
    let bytes = serde_json::to_vec(&envelope).unwrap().len();
    assert!(pages
        .accept(&context(), "preview", &page, bytes - 1)
        .is_err());
    assert_eq!(pages, before);
    pages.accept(&context(), "preview", &page, bytes).unwrap();
}

#[test]
fn human_hd_labels_are_one_based_but_the_exact_wire_owner_and_source_stay_zero_based() {
    assert_eq!(
        parse("purchase 1 general skill athletics").unwrap(),
        AdvancementInput::Preview(AdvancementProposal::Purchase {
            purchase: PurchaseOperation {
                owner: 0,
                pool: "general".into(),
                selection: PurchaseChoice::SkillRank {
                    skill: "athletics".into()
                }
            }
        })
    );
    assert_eq!(
        parse("remove 1 2").unwrap(),
        AdvancementInput::Preview(proposal())
    );
    assert!(parse("remove 0 2").is_err());
    assert!(parse("purchase 257 general skill athletics").is_ok());
    assert_eq!(
        parse("confirm using carried").unwrap(),
        AdvancementInput::Confirm(Some("carried".into()))
    );
}

#[test]
fn consequences_include_death_health_recovery_resources_and_condition_loss() {
    let mut pages = opened();
    install(&mut pages, preview_page(), "preview").unwrap();
    let lines = pages.lines().join("\n");
    for fact in [
        "DEATH",
        "Health",
        "injury",
        "recovery",
        "stamina",
        "focus",
        "mana",
        "condition",
        "33",
        "removed",
        "refund",
    ] {
        assert!(lines.contains(fact), "missing {fact}: {lines}");
    }
}

#[test]
fn a_fresh_respec_cannot_replace_active_or_paused_work() {
    for paused in [false, true] {
        let mut pages = AdvancementPages::default();
        let mut page = overview_page();
        let AdvancementEntry::Overview { value } = &mut page.entries[0] else {
            unreachable!()
        };
        if paused {
            value.respec = Some(OwnerRespecProgress {
                intention: IntentionId("respec-1".into()),
                basis: basis(),
                replacement: match proposal() {
                    AdvancementProposal::Replacement { replacement } => replacement,
                    _ => unreachable!(),
                },
                active: false,
                total_ticks: 10,
                remaining_ticks: 7,
            });
        } else {
            value.other_preparation = true;
        }
        install(&mut pages, page, "open").unwrap();
        install(&mut pages, preview_page(), "preview").unwrap();
        assert!(pages
            .confirm(
                &context(),
                &crate::test_support::observation(),
                SupplySelection::Carried
            )
            .is_err());
    }
}

#[test]
fn respec_exact_supplies_reuse_known_touch_ground_facts_and_one_cost_ordinal() {
    let pages = opened();
    let mut view = crate::test_support::observation();
    let ground = GroundItemView {
        reachable: false,
        supply_reachable: true,
        position: view.position,
        item: ItemView {
            id: ItemTarget::from_digest([31; 32]),
            class: ItemClass::Misc,
            quantity: 2,
            appearance: "ration".into(),
            identified: true,
            name: "ration source".into(),
            description: String::new(),
            asset: None,
            known_supply: Some(SupplyView {
                id: "ration".into(),
                name: "Ration".into(),
            }),
        },
    };
    let item = ground.item.id;
    view.ground_items = vec![ground];
    view.inventory.clear();
    let expected = SupplySelection::Explicit {
        items: vec![SelectedSupply {
            cost_ordinal: 0,
            item,
        }],
    };
    assert_eq!(
        pages.supplies_from_text(&view, "ration source").unwrap(),
        expected
    );
    assert!(pages.validate_supplies(&view, &expected).is_ok());
    assert!(pages
        .validate_supplies(
            &view,
            &SupplySelection::Explicit {
                items: vec![SelectedSupply {
                    cost_ordinal: 1,
                    item
                }]
            }
        )
        .is_err());
    assert!(pages
        .validate_supplies(
            &view,
            &SupplySelection::Explicit {
                items: vec![
                    SelectedSupply {
                        cost_ordinal: 0,
                        item
                    },
                    SelectedSupply {
                        cost_ordinal: 0,
                        item
                    }
                ]
            }
        )
        .is_err());
    view.ground_items[0].supply_reachable = false;
    assert!(pages.validate_supplies(&view, &expected).is_err());
    assert!(pages
        .validate_supplies(&view, &SupplySelection::Carried)
        .is_ok());
}

#[test]
fn received_loss_quiet_and_perceived_threat_are_disclosed_refusals_not_hidden_inferences() {
    for threat in [false, true] {
        let mut pages = AdvancementPages::default();
        let mut page = overview_page();
        let AdvancementEntry::Overview { value } = &mut page.entries[0] else {
            unreachable!()
        };
        value.perceived_threat = threat;
        value.received_loss_remaining = if threat { 0 } else { 12 };
        install(&mut pages, page, "open").unwrap();
        install(&mut pages, preview_page(), "preview").unwrap();
        assert!(pages
            .confirm(
                &context(),
                &crate::test_support::observation(),
                SupplySelection::Carried
            )
            .unwrap_err()
            .contains("perceived danger"));
    }
}

#[test]
fn resume_and_cancel_name_only_the_exact_currently_permitted_respec_intention() {
    let context = context();
    let id = IntentionId("retained-respec".into());
    let mut view = crate::test_support::observation();
    view.tick = context.cursor.tick;
    let snapshot = Snapshot {
        context: context.input.stream.clone(),
        readiness: Readiness {
            revision: context.input.readiness_revision,
            admission: false,
            resume: vec![id.clone()],
            cancel: vec![id.clone()],
        },
        intentions: vec![IntentionStatus {
            actor: context.actor,
            branch: context.branch.clone(),
            intention: id.clone(),
            entry_id: EntryId("admission".into()),
            phase: IntentionPhase::Paused,
        }],
        travel: None,
        actor: context.actor,
        branch: context.branch.clone(),
        cursor: context.cursor,
        state: std::sync::Arc::new(StateView {
            wizard_game: false,
            revision: context.revision,
            observation: view,
        }),
        has_control: true,
        history: HistoryPage {
            entries: vec![],
            older_before: None,
        },
    };
    let state = crate::ClientState::from_snapshot(snapshot).unwrap();
    let mut pages = AdvancementPages::default();
    let mut page = overview_page();
    let AdvancementEntry::Overview { value } = &mut page.entries[0] else {
        unreachable!()
    };
    let mut original = basis();
    original.reset = 0;
    value.ready = false;
    value.respec = Some(OwnerRespecProgress {
        intention: id.clone(),
        basis: original,
        replacement: match proposal() {
            AdvancementProposal::Replacement { replacement } => replacement,
            _ => unreachable!(),
        },
        active: false,
        total_ticks: 600,
        remaining_ticks: 300,
    });
    install(&mut pages, page, "overview").unwrap();
    assert!(
        matches!(pages.request(AdvancementInput::Resume, &state).unwrap(), Some(Request::Command { command: Command::ResumeIntention { intention, .. }, .. }) if intention == id)
    );
    assert!(
        matches!(pages.request(AdvancementInput::Cancel, &state).unwrap(), Some(Request::Command { command: Command::CancelIntention { intention, step: None, .. }, .. }) if intention == id)
    );
}

#[test]
fn empty_sections_keep_explicit_end_and_wrong_catalog_rows_are_rejected() {
    let mut pages = opened();
    let mut page = AdvancementPage {
        basis: basis(),
        revision: 6,
        query: AdvancementQuery::Ledger {
            basis: basis(),
            offset: 0,
        },
        total: 0,
        next: None,
        entries: vec![],
    };
    install(&mut pages, page.clone(), "empty").unwrap();
    assert!(pages.next(&context()).is_err());
    page.next = Some(0);
    pages
        .track(&context(), "malformed", 6, &page.query)
        .unwrap();
    let before = pages.clone();
    assert!(pages
        .accept(&context(), "malformed", &page, MAX_ADVANCEMENT_PAGE_BYTES)
        .is_err());
    assert_eq!(pages, before);
    let mut pages = opened();
    let page = AdvancementPage {
        basis: basis(),
        revision: 6,
        query: AdvancementQuery::Catalog {
            basis: basis(),
            catalog: AdvancementKind::SkillRank,
            offset: 0,
        },
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Catalog {
            value: AdvancementCatalogRow {
                form_choices: Vec::new(),
                selection: PurchaseChoice::Talent {
                    talent: "arbitrary".into(),
                },
                name: "Arbitrary".into(),
            },
        }],
    };
    assert!(install(&mut pages, page, "wrong-catalog").is_err());
}

#[test]
fn complete_envelope_budget_counts_escaped_names_and_the_retained_query_echo() {
    let mut pages = opened();
    let page = AdvancementPage {
        basis: basis(),
        revision: 6,
        query: AdvancementQuery::Catalog {
            basis: basis(),
            catalog: AdvancementKind::Talent,
            offset: 0,
        },
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Catalog {
            value: AdvancementCatalogRow {
                form_choices: Vec::new(),
                selection: PurchaseChoice::Talent {
                    talent: "arbitrary".into(),
                },
                name: "\"\\".repeat(100),
            },
        }],
    };
    pages.track(&context(), "escaped", 6, &page.query).unwrap();
    let message = ServerMessage::AdvancementPage {
        context: context(),
        request_id: "escaped".into(),
        page: Box::new(page.clone()),
    };
    let bytes = serde_json::to_vec(&message).unwrap().len();
    assert!(bytes > serde_json::to_vec(&page.entries).unwrap().len() + 200);
    let before = pages.clone();
    assert!(pages
        .accept(&context(), "escaped", &page, bytes - 1)
        .is_err());
    assert_eq!(pages, before);
    pages.accept(&context(), "escaped", &page, bytes).unwrap();
}

#[test]
fn a_detail_count_cannot_disagree_with_its_exact_summary() {
    let mut pages = opened();
    install(&mut pages, preview_page(), "preview").unwrap();
    let page = AdvancementPage {
        basis: basis(),
        revision: 6,
        query: AdvancementQuery::Preview {
            basis: basis(),
            proposal: proposal(),
            section: AdvancementPreviewSection::RemovedConditions,
            offset: 0,
        },
        total: 0,
        next: None,
        entries: vec![],
    };
    pages.track(&context(), "details", 6, &page.query).unwrap();
    let before = pages.clone();
    assert!(pages
        .accept(&context(), "details", &page, MAX_ADVANCEMENT_PAGE_BYTES)
        .is_err());
    assert_eq!(pages, before);
}

#[test]
fn form_purchase_and_replacement_grammar_keep_the_exact_root_and_acquisition_operand() {
    for text in [
        "purchase 1 advancement talent evocation_initiate form spark_lance.cold",
        "replace 1 2 advancement talent evocation_initiate form spark_lance.cold",
    ] {
        let AdvancementInput::Preview(proposal) = parse(text).unwrap() else {
            panic!("preview")
        };
        let value = serde_json::to_value(&proposal).unwrap();
        let selection = if value["kind"] == "purchase" {
            &value["purchase"]["selection"]
        } else {
            &value["replacement"]["replacement"]["selection"]
        };
        assert_eq!(
            selection,
            &serde_json::json!({"kind":"talent_form","talent":"evocation_initiate","form":"spark_lance.cold"})
        );
        assert!(
            proposal_label(&proposal).contains("talent evocation_initiate form spark_lance.cold")
        );
    }
    for text in [
        "purchase 1 advancement skill lore form spark_lance.cold",
        "purchase 1 advancement talent root form",
        "purchase 1 advancement talent root form BAD",
    ] {
        assert!(parse(text).is_err());
    }
}

#[test]
fn form_catalog_presents_each_disclosed_option_without_flattening_roots() {
    let entry: AdvancementEntry = serde_json::from_value(serde_json::json!({"kind":"catalog","value":{
        "selection":{"kind":"talent","talent":"evocation_initiate"},"name":"Evocation Initiate",
        "form_choices":[{"id":"spark_lance.cold","name":"Cold Lance"},{"id":"spark_lance.fire","name":"Fire Lance"}]}})).unwrap();
    let mut rows = Vec::new();
    render_entry(&entry, &mut rows);
    assert_eq!(rows.len(), 3);
    assert!(rows[1].contains("Cold Lance") && rows[1].contains("spark_lance.cold"));
    assert!(rows[2].contains("Fire Lance") && rows[2].contains("spark_lance.fire"));
}

#[test]
fn form_confirmation_retains_the_exact_preview_and_rejects_a_substituted_form() {
    let make_proposal = |form: &str| AdvancementProposal::Purchase {
        purchase: PurchaseOperation {
            owner: 0,
            pool: "advancement".into(),
            selection: PurchaseChoice::TalentForm {
                talent: "root".into(),
                form: form.into(),
            },
        },
    };
    let selected = make_proposal("choice.cold");
    let mut page = preview_page();
    if let AdvancementQuery::Preview { proposal, .. } = &mut page.query {
        *proposal = selected.clone();
    }
    if let AdvancementEntry::Summary { value } = &mut page.entries[0] {
        value.proposal = selected.clone();
    }
    let mut pages = opened();
    pages
        .track(&context(), "form-preview", 6, &page.query)
        .unwrap();
    let before = pages.clone();
    let mut changed = page.clone();
    if let AdvancementQuery::Preview { proposal, .. } = &mut changed.query {
        *proposal = make_proposal("choice.fire");
    }
    if let AdvancementEntry::Summary { value } = &mut changed.entries[0] {
        value.proposal = make_proposal("choice.fire");
    }
    assert!(pages
        .accept(
            &context(),
            "form-preview",
            &changed,
            MAX_ADVANCEMENT_PAGE_BYTES
        )
        .is_err());
    assert_eq!(pages, before);
    pages
        .accept(
            &context(),
            "form-preview",
            &page,
            MAX_ADVANCEMENT_PAGE_BYTES,
        )
        .unwrap();
    let command = pages
        .confirm(
            &context(),
            &crate::test_support::observation(),
            SupplySelection::Carried,
        )
        .unwrap();
    let Command::Act {
        action: Action::Purchase {
            purchase,
            basis: actual_basis,
        },
        expected_revision,
    } = command
    else {
        panic!("exact purchase")
    };
    assert_eq!(expected_revision, 6);
    assert_eq!(*actual_basis, basis());
    assert_eq!(AdvancementProposal::Purchase { purchase }, selected);
}

#[test]
fn growth_source_controls_keep_exact_preview_and_basis_without_client_xp() {
    for (text, source) in [
        ("source racial", GrowthSource::Racial {}),
        (
            "source class warrior",
            GrowthSource::Class {
                class: "warrior".into(),
            },
        ),
    ] {
        let proposal = AdvancementProposal::GrowthSource {
            source: source.clone(),
        };
        assert_eq!(
            parse(text).unwrap(),
            AdvancementInput::Preview(proposal.clone())
        );
        let mut pages = opened();
        let mut preview = preview_page();
        let AdvancementQuery::Preview {
            proposal: requested,
            ..
        } = &mut preview.query
        else {
            panic!("preview")
        };
        *requested = proposal.clone();
        let AdvancementEntry::Summary { value } = &mut preview.entries[0] else {
            panic!("summary")
        };
        value.proposal = proposal;
        install(&mut pages, preview, "growth-preview").unwrap();
        assert_eq!(
            pages
                .confirm(
                    &context(),
                    &crate::test_support::observation(),
                    SupplySelection::Carried
                )
                .unwrap(),
            Command::Act {
                expected_revision: 6,
                action: Action::ChooseGrowthSource {
                    basis: Box::new(basis()),
                    source
                }
            }
        );
        let mut reset = context();
        reset.input.stream.epoch += 1;
        assert!(pages
            .confirm(
                &reset,
                &crate::test_support::observation(),
                SupplySelection::Carried
            )
            .is_err());
    }
    assert!(parse("source class INVALID").is_err());
    assert!(parse("source racial 100").is_err());
    assert!(opened().lines().iter().any(|line| line.contains("XP:")));
}

#[test]
fn hit_die_restoration_overview_distinguishes_applied_from_earned_and_explains_missing_source() {
    let mut value = overview();
    value.growth.highest_applied_tier = 3;
    value.growth.highest_earned_tier = 5;
    value.growth.restorable_hit_dice = 1;
    value.growth.restoration_blocker = Some(HitDieRestorationBlocker::MissingSource {});
    let mut rows = vec![];
    render_entry(
        &AdvancementEntry::Overview {
            value: Box::new(value.clone()),
        },
        &mut rows,
    );
    let text = rows.join("\n");
    assert!(text.contains("highest earned HD 5"));
    assert!(text.contains("Restorable Hit Dice 1; highest applied HD 3"));
    assert!(text.contains("Select a growth/restoration source"));
    value.growth.restoration_blocker = Some(HitDieRestorationBlocker::MissingHealthRoot {});
    rows.clear();
    render_entry(
        &AdvancementEntry::Overview {
            value: Box::new(value),
        },
        &mut rows,
    );
    assert!(rows.join("\n").contains("no original Health root"));
}

#[test]
fn wide_scalar_hd_ordinals_keep_the_full_u64_range_and_bounded_purchases() {
    let input = parse("remove 18446744073709551615 256").unwrap();
    let AdvancementInput::Preview(proposal) = input else {
        panic!("expected preview")
    };
    let AdvancementProposal::Replacement { ref replacement } = proposal else {
        panic!("expected replacement")
    };
    assert_eq!(replacement.source.owner, u64::MAX - 1);
    assert_eq!(replacement.source.index, 255);
    assert!(proposal_label(&proposal).contains("18446744073709551615"));
    for text in [
        "remove 0 1",
        "remove 18446744073709551616 1",
        "remove 1 257",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

#[test]
fn wide_scalar_programmatic_invalid_owners_have_safe_labels() {
    let replacement = AdvancementProposal::Replacement {
        replacement: ReplacementOperation {
            source: ReplacementSource {
                owner: u64::MAX,
                index: 0,
            },
            replacement: None,
        },
    };
    assert!(proposal_label(&replacement).contains("invalid"));
    let purchase = AdvancementProposal::Purchase {
        purchase: PurchaseOperation {
            owner: u64::MAX,
            pool: "general".into(),
            selection: PurchaseChoice::SkillRank {
                skill: "athletics".into(),
            },
        },
    };
    assert!(proposal_label(&purchase).contains("invalid"));
}
