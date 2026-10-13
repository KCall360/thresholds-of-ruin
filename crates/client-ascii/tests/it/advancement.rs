use tor_client_ascii::{App, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn app(control: bool) -> App {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":control,"resume":[],"cancel":[]},
        "context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test",
        "cursor":{"sequence":"0","tick":"0"},"has_control":control,"intentions":[],
        "history":{"entries":[],"older_before":null},"state":{"wizard_game":false,"revision":"1","observation":test_support::observation()}
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
    let before = app.state.as_ref().unwrap().snapshot();
    assert!(matches!(
        super::request(key(&mut app, Key::Advancement)),
        Some(Request::Command {
            command: Command::Advancement {
                query: AdvancementQuery::Overview {},
                expected_revision: 1
            },
            ..
        })
    ));
    assert!(app.advancement_open);
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
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
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
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
            value: AdvancementOverview {
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
                respec: None,
            },
        }],
    }
}

#[test]
fn a_network_validated_private_page_cannot_cross_the_ui_control_loss_queue_boundary() {
    let mut app = app(true);
    assert!(matches!(
        key(&mut app, Key::Advancement),
        Effect::Request(_)
    ));
    let context = app.state.as_ref().unwrap().snapshot().reply_context();
    let page = page(&context);
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
    let old = app.state.as_ref().unwrap().snapshot().reply_context();
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
    let fresh = app.state.as_ref().unwrap().snapshot();
    assert_eq!(fresh.readiness.revision, old.input.readiness_revision);
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
    assert_eq!(app.state.as_ref().unwrap().snapshot(), fresh);

    for id in ["fresh-overview", "repeated-overview"] {
        app.input(Input::Text {
            text: "overview".into(),
        });
        let Some(Request::Command {
            command:
                Command::Advancement {
                    expected_revision,
                    query: AdvancementQuery::Overview {},
                },
            ..
        }) = super::request(key(&mut app, Key::Enter))
        else {
            panic!("explicit overview must use the latest disclosed revision")
        };
        assert_eq!(expected_revision, fresh.state.revision);
        // Repeated Enter while that page is pending must not enqueue another request.
        assert_eq!(key(&mut app, Key::Enter), Effect::None);
        let context = fresh.reply_context();
        app.bind_advancement_request(id).unwrap();
        app.show_advancement_page(&context, id, &page(&context))
            .unwrap();
        app.ready();
        assert!(app.connected && app.advancement_open);
        assert_eq!(app.state.as_ref().unwrap().snapshot(), fresh);
    }

    // A close wins over a late page.
    app.input(Input::Text {
        text: "overview".into(),
    });
    assert!(matches!(key(&mut app, Key::Enter), Effect::Request(_)));
    app.bind_advancement_request("closing").unwrap();
    assert_eq!(key(&mut app, Key::Escape), Effect::None);
    assert!(!app.advancement_open);
    let context = fresh.reply_context();
    assert!(app
        .show_advancement_page(&context, "closing", &page(&context))
        .is_err());
    assert!(!app.advancement_open);

    // Disconnect with private content and a pending preview binding still live,
    // rather than checking an already-closed screen that has nothing to clear.
    app.ready();
    assert!(matches!(
        key(&mut app, Key::Advancement),
        Effect::Request(_)
    ));
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
    let mut reconnected = fresh;
    reconnected.context.epoch += 1;
    app.replace_snapshot(reconnected).unwrap();
    app.ready();
    assert!(app.connected && !app.advancement_open);
    assert!(app.advancement_command.is_empty());
    assert!(app.supply_selection.is_none());
    assert!(!app.advancement_rows().join(" ").contains("deception"));
}
