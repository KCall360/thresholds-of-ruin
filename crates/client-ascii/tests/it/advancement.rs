use tor_client_ascii::{App, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn app(control: bool) -> App {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":control,"resume":[],"cancel":[]},
        "context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test",
        "cursor":{"sequence":"0","tick":"0"},"has_control":control,"intentions":[],
        "history":{"entries":[],"older_before":null},"state":{"snapshot":"fixture-snapshot","wizard_game":false,"revision":"1","observation":test_support::observation()}
    })).unwrap();
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.role = AccessRole::Player;
    app.ready();
    app.messages.skip();
    app
}
fn key(app: &mut App, key: Key) -> Effect {
    app.input(Input::Key { key })
}

#[test]
fn ordinary_advancement_opens_only_for_the_current_controller_and_never_uses_wizard() {
    let mut app = app(true);
    let before = app.state.as_ref().unwrap().session().clone();
    let Effect::OpenOverview(plan) = key(&mut app, Key::Advancement) else {
        panic!("opening advancement must first demand a fresh overview picture")
    };
    assert_eq!(plan.observation_request(), Request::Observe);
    assert!(app.bind_advancement_request("premature").is_err());
    assert!(app.advancement_open);
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
    let mut observer = self::app(false);
    assert_eq!(key(&mut observer, Key::Advancement), Effect::None);
    assert!(!observer.advancement_open);
}

#[test]
fn closing_or_disconnecting_advancement_discards_drafts_and_does_not_quit() {
    let mut app = app(true);
    key(&mut app, Key::Advancement);
    app.ready();
    app.input(Input::Text {
        text: "purchase 1 general skill athletics".into(),
    });
    assert!(!app.advancement_command.is_empty());
    assert_eq!(key(&mut app, Key::Escape), Effect::None);
    assert!(!app.advancement_open);
    assert!(app.advancement_command.is_empty());
    key(&mut app, Key::Advancement);
    app.disconnect("Disconnected".into());
    assert!(!app.advancement_open);
    assert!(app
        .advancement_rows()
        .iter()
        .all(|row| !row.contains("athletics")));
}

#[test]
fn owner_control_loss_clears_the_screen_even_when_role_and_actor_are_unchanged() {
    let mut app = app(true);
    key(&mut app, Key::Advancement);
    let mut snapshot = app.state.as_ref().unwrap().reset_snapshot().unwrap();
    snapshot.has_control = false;
    snapshot.readiness.admission = false;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(!app.advancement_open);
    assert_eq!(app.role, AccessRole::Player);
}

fn page(context: &ReplyContext) -> AdvancementPage {
    let content = |id: &str| AdvancementContentRef {
        id: id.into(),
        name: id.into(),
    };
    AdvancementPage {
        basis: AdvancementBasis {
            actor: context.actor,
            branch: context.branch.clone(),
            stream: context.input.stream.stream.clone(),
            reset: context.input.stream.epoch,
            build_epoch: 1,
        },
        revision: context.revision,
        query: AdvancementQuery::Overview {},
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Overview {
            value: Box::new(AdvancementOverview {
                strength_acquired: 0,
                growth: AdvancementGrowth {
                    body_package_hit_dice: None,
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
                starting_skills: [content("deception"), content("persuasion")],
                points: AdvancementPoints {
                    earned: 4,
                    spent: 0,
                    unspent: 4,
                },
                timings: AdvancementTimings {
                    purchase_recovery: 0,
                    respec_prepare: 600,
                    respec_recovery: 0,
                    received_loss_interval: 600,
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
                locked: false,
                respec: None,
            }),
        }],
    }
}

#[test]
fn a_network_validated_private_page_cannot_cross_the_ui_control_loss_queue_boundary() {
    let mut app = app(true);
    assert!(matches!(
        key(&mut app, Key::Advancement),
        Effect::OpenOverview(_)
    ));
    let context = app.state.as_ref().unwrap().reply_context();
    let page = page(&context);
    // The worker has delivered its Observe picture before preparing this query.
    app.prepare_overview(app.state.as_ref().unwrap().state().revision)
        .unwrap();
    app.bind_advancement_request("pending-owner-page").unwrap();
    let mut network = tor_client_common::advancement::AdvancementPages::default();
    network
        .track(
            &context,
            "pending-owner-page",
            context.revision,
            &page.query,
        )
        .unwrap();
    network
        .validate_reply(
            &context,
            "pending-owner-page",
            &page,
            MAX_ADVANCEMENT_PAGE_BYTES,
        )
        .unwrap();
    // The network has accepted this reply, but the UI consumes the newer
    // ordered control update before a delayed queued presentation event.
    app.update(StreamUpdate {
        context: context.input.stream.clone(),
        actor: context.actor,
        branch: context.branch.clone(),
        cursor: StreamCursor {
            sequence: context.cursor.sequence + 1,
            tick: context.cursor.tick,
        },
        body: UpdateBody::Control { has_control: false },
    })
    .unwrap();
    assert_eq!(app.role, AccessRole::Player);
    assert_eq!(
        app.state.as_ref().unwrap().state().observation.actor,
        context.actor
    );
    assert!(!app.advancement_open);
    assert!(app
        .show_advancement_page(&context, "pending-owner-page", &page)
        .is_err());
    assert!(app.advancement_command.is_empty());
    assert!(!app.advancement_rows().join(" ").contains("deception"));
    assert!(app.supply_selection.is_none());
}

#[test]
fn a_stale_queued_query_preserves_focus_and_requires_explicit_fresh_navigation() {
    let mut app = app(true);
    key(&mut app, Key::Advancement);
    let old = app.state.as_ref().unwrap().reply_context();
    // The worker has delivered its Observe picture before preparing this query.
    app.prepare_overview(app.state.as_ref().unwrap().state().revision)
        .unwrap();
    app.bind_advancement_request("overview").unwrap();
    app.show_advancement_page(&old, "overview", &page(&old))
        .unwrap();
    app.ready();
    app.input(Input::Text {
        text: "remove 1 1".into(),
    });
    assert!(matches!(key(&mut app, Key::Enter), Effect::Request(_)));

    // The worker publishes a newer observation with the same authority
    // generation before refusing the queued, old-revision preview.
    let mut state = app.state.as_ref().unwrap().state().clone();
    state.revision += 1;
    state.snapshot = tor_protocol::SnapshotId(format!("fixture-display-{}", state.revision));
    app.update(StreamUpdate {
        context: old.input.stream.clone(),
        actor: old.actor,
        branch: old.branch.clone(),
        cursor: StreamCursor {
            sequence: old.cursor.sequence + 1,
            tick: old.cursor.tick,
        },
        body: UpdateBody::Observation {
            state: state.into(),
            event: None,
        },
    })
    .unwrap();
    let fresh = app.state.as_ref().unwrap().session().clone();
    let fresh_context = app.state.as_ref().unwrap().reply_context();
    let fresh_reset = app.state.as_ref().unwrap().reset_snapshot().unwrap();
    assert_eq!(
        fresh_context.input.readiness_revision,
        old.input.readiness_revision
    );
    app.refused(
        "Advancement changed; request was not sent. Request overview and explicitly preview again."
            .into(),
    );
    app.ready();
    assert!(app.connected && app.advancement_open && !app.busy);
    assert!(app.status.contains("not sent"));
    assert!(app.supply_selection.is_none());
    app.input(Input::Text {
        text: "confirm".into(),
    });
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(app.status.contains("stale"));
    assert_eq!(app.state.as_ref().unwrap().session().clone(), fresh);

    for id in ["fresh-overview", "repeated-overview"] {
        app.input(Input::Text {
            text: "overview".into(),
        });
        assert!(matches!(key(&mut app, Key::Enter), Effect::OpenOverview(_)));
        assert!(
            app.bind_advancement_request("too-early").is_err(),
            "no page may bind before Observe completes"
        );
        app.prepare_overview(fresh_context.revision).unwrap();
        // Repeated Enter while that page is pending must not enqueue another request.
        assert_eq!(key(&mut app, Key::Enter), Effect::None);
        let context = fresh_context.clone();
        app.bind_advancement_request(id).unwrap();
        app.show_advancement_page(&context, id, &page(&context))
            .unwrap();
        app.ready();
        assert!(app.connected && app.advancement_open);
        assert_eq!(app.state.as_ref().unwrap().session().clone(), fresh);
    }

    // A close wins over a late page.
    app.input(Input::Text {
        text: "overview".into(),
    });
    assert!(matches!(key(&mut app, Key::Enter), Effect::OpenOverview(_)));
    // The worker has delivered its Observe picture before preparing this query.
    app.prepare_overview(app.state.as_ref().unwrap().state().revision)
        .unwrap();
    app.bind_advancement_request("closing").unwrap();
    assert_eq!(key(&mut app, Key::Escape), Effect::None);
    assert!(!app.advancement_open);
    let context = fresh_context.clone();
    assert!(app
        .show_advancement_page(&context, "closing", &page(&context))
        .is_err());
    assert!(!app.advancement_open);

    // Disconnect with private content and a pending preview binding still live,
    // rather than checking an already-closed screen that has nothing to clear.
    app.ready();
    assert!(matches!(
        key(&mut app, Key::Advancement),
        Effect::OpenOverview(_)
    ));
    // The worker has delivered its Observe picture before preparing this query.
    app.prepare_overview(app.state.as_ref().unwrap().state().revision)
        .unwrap();
    app.bind_advancement_request("reopened").unwrap();
    app.show_advancement_page(&context, "reopened", &page(&context))
        .unwrap();
    app.ready();
    app.input(Input::Text {
        text: "remove 1 1".into(),
    });
    assert!(matches!(key(&mut app, Key::Enter), Effect::Request(_)));
    assert!(app.connected && app.advancement_open && app.busy);
    assert!(app.advancement_rows().join(" ").contains("deception"));
    app.disconnect("Disconnected".into());
    assert!(!app.connected && !app.advancement_open && !app.busy);
    assert!(app
        .bind_advancement_request("disconnected-preview")
        .is_err());
    assert!(!app.advancement_rows().join(" ").contains("deception"));
    let mut reconnected = fresh_reset;
    reconnected.context.epoch += 1;
    app.replace_snapshot(reconnected).unwrap();
    app.ready();
    assert!(app.connected && !app.advancement_open);
    assert!(app.advancement_command.is_empty());
    assert!(app.supply_selection.is_none());
    assert!(!app.advancement_rows().join(" ").contains("deception"));
}

#[test]
fn expertise_catalog_uses_shared_ordinary_query_without_local_rules() {
    let mut app = app(true);
    key(&mut app, Key::Advancement);
    let context = app.state.as_ref().unwrap().reply_context();
    // The worker has delivered its Observe picture before preparing this query.
    app.prepare_overview(app.state.as_ref().unwrap().state().revision)
        .unwrap();
    app.bind_advancement_request("overview").unwrap();
    app.show_advancement_page(&context, "overview", &page(&context))
        .unwrap();
    app.ready();
    app.input(Input::Text {
        text: "expertises".into(),
    });
    assert!(matches!(
        super::request(key(&mut app, Key::Enter)),
        Some(Request::Command {
            command: Command::Advancement {
                query: AdvancementQuery::Catalog {
                    catalog: AdvancementKind::Expertise,
                    offset: 0,
                    ..
                },
                ..
            },
            ..
        })
    ));
}

#[test]
fn intellect_expertise_navigation_and_choice_use_ordinary_shared_preview() {
    for text in ["intellect-expertises", "intellect-expertise 3 spear"] {
        let mut app = app(true);
        key(&mut app, Key::Advancement);
        let context = app.state.as_ref().unwrap().reply_context();
        // The worker has delivered its Observe picture before preparing this query.
        app.prepare_overview(app.state.as_ref().unwrap().state().revision)
            .unwrap();
        app.bind_advancement_request("overview").unwrap();
        app.show_advancement_page(&context, "overview", &page(&context))
            .unwrap();
        app.ready();
        app.input(Input::Text { text: text.into() });
        let Some(Request::Command {
            command: Command::Advancement { query, .. },
            ..
        }) = super::request(key(&mut app, Key::Enter))
        else {
            panic!("ordinary advancement query")
        };
        if text == "intellect-expertises" {
            assert!(matches!(
                query,
                AdvancementQuery::IntellectExpertises { offset: 0, .. }
            ));
        } else {
            assert!(
                matches!(query, AdvancementQuery::Preview { proposal: AdvancementProposal::IntellectExpertise { choice: IntellectExpertiseOperation { threshold: 3, expertise } }, section: AdvancementPreviewSection::Summary, offset: 0, .. } if expertise == "spear")
            );
        }
    }
}
