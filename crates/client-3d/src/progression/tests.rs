use super::*;
use bevy::input::ButtonState;
use tor_protocol::*;

fn app() -> (App, tokio::sync::mpsc::Receiver<crate::net::Input>) {
    let (net, receiver) = crate::net::test_channel();
    let mut view = crate::test_support::view(0);
    view.has_control = true;
    view.can_act = true;
    let context = view.shown.clone();
    let basis = AdvancementBasis {
        actor: context.actor,
        branch: context.branch.clone(),
        stream: context.input.stream.stream.clone(),
        reset: context.input.stream.epoch,
        build_epoch: 3,
    };
    let row = AdvancementBalanceRow {
        owner: 0,
        pool: AdvancementContentRef {
            id: "practice".into(),
            name: "Practice".into(),
        },
        granted: 2,
        spent: 0,
        unspent: 2,
        allowed: vec![AdvancementKind::SkillRank],
    };
    let page = AdvancementPage {
        basis: basis.clone(),
        revision: 0,
        query: AdvancementQuery::Balances { basis, offset: 0 },
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Balance { value: row }],
    };
    page.validate().unwrap();
    let mut app = App::new();
    app.insert_resource(net)
        .insert_resource(Game {
            view: Some(view),
            screen: Some(Screen::Advancement),
            advancement_view: tor_client_3d_model::AdvancementView {
                context: Some(context),
                page: Some(page),
                ..default()
            },
            ..default()
        })
        .init_resource::<State>()
        .add_event::<KeyboardInput>()
        .add_systems(Startup, crate::character::setup)
        .add_systems(
            Update,
            (
                sync,
                controls,
                crate::screen::typing,
                crate::character::show,
                draw,
            )
                .chain(),
        );
    app.update();
    (app, receiver)
}
fn press(app: &mut App, key: Key) {
    app.world_mut().send_event(KeyboardInput {
        logical_key: key,
        key_code: KeyCode::Enter,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.update();
}
fn preview(app: &mut App) {
    let mut game = app.world_mut().resource_mut::<Game>();
    let basis = game.advancement_view.page.as_ref().unwrap().basis.clone();
    let proposal = AdvancementProposal::Purchase {
        purchase: PurchaseOperation {
            owner: 0,
            pool: "practice".into(),
            selection: PurchaseChoice::SkillRank {
                skill: "perception".into(),
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
    game.advancement_view.page = Some(AdvancementPage {
        basis: basis.clone(),
        revision: 0,
        query: AdvancementQuery::Preview {
            basis: basis.clone(),
            proposal: proposal.clone(),
            section: AdvancementPreviewSection::Summary,
            offset: 0,
        },
        total: 1,
        next: None,
        entries: vec![AdvancementEntry::Summary {
            value: summary.clone(),
        }],
    });
    game.advancement_view
        .page
        .as_ref()
        .unwrap()
        .validate()
        .unwrap();
    game.advancement_view.confirmation = Some(tor_client_common::advancement::Confirmation {
        basis,
        revision: 0,
        proposal,
        summary,
    });
    game.advancement_serial += 1;
}

#[test]
fn empty_enter_chooses_a_pool_then_requests_a_purchase_preview_without_committing() {
    let (mut app, mut receiver) = app();
    press(&mut app, Key::Enter);
    let first = receiver.try_recv().unwrap();
    assert!(matches!(first.command, Cmd::Advancement(text) if text == "skills"));
    let (net, mut receiver) = crate::net::test_channel();
    app.insert_resource(net);
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.busy = false;
        let page = game.advancement_view.page.as_mut().unwrap();
        page.query = AdvancementQuery::Catalog {
            basis: page.basis.clone(),
            catalog: AdvancementKind::SkillRank,
            offset: 0,
        };
        page.entries = vec![AdvancementEntry::Catalog {
            value: AdvancementCatalogRow {
                selection: PurchaseChoice::SkillRank {
                    skill: "perception".into(),
                },
                name: "Perception".into(),
                form_choices: vec![],
            },
        }];
        game.advancement_serial += 1;
    }
    app.update();
    press(&mut app, Key::Enter);
    let purchase = receiver.try_recv().unwrap();
    assert_eq!(purchase.shown, first.shown);
    assert!(
        matches!(purchase.command, Cmd::Advancement(text) if text == "purchase 1 practice skill perception")
    );
    assert!(
        receiver.try_recv().is_err(),
        "one selection cannot send confirm too"
    );
    assert!(app
        .world()
        .resource::<Game>()
        .advancement_view
        .confirmation
        .is_none());
}

#[test]
fn only_explicit_f7_confirms_the_current_ready_preview_and_preserves_command_drafts() {
    let (mut app, mut receiver) = app();
    preview(&mut app);
    app.update();
    press(&mut app, Key::Enter);
    assert!(
        receiver.try_recv().is_err(),
        "empty Enter never confirms a preview"
    );
    app.world_mut().resource_mut::<Game>().typed = "purchase 1 practice skill perception".into();
    press(&mut app, Key::F7);
    assert!(receiver.try_recv().is_err());
    assert_eq!(
        app.world().resource::<Game>().typed,
        "purchase 1 practice skill perception"
    );
    app.world_mut().resource_mut::<Game>().typed.clear();
    press(&mut app, Key::F7);
    assert!(
        matches!(receiver.try_recv().unwrap().command, Cmd::Advancement(text) if text == "confirm")
    );
    press(&mut app, Key::F7);
    assert!(
        receiver.try_recv().is_err(),
        "busy repeat cannot double commit"
    );
}

#[test]
fn stale_unowned_not_ready_closed_and_old_button_generations_cannot_confirm() {
    for rejected in 0..6 {
        let (mut app, mut receiver) = app();
        preview(&mut app);
        app.update();
        {
            let mut game = app.world_mut().resource_mut::<Game>();
            match rejected {
                0 => game.view.as_mut().unwrap().shown.revision += 1,
                1 => game.view.as_mut().unwrap().has_control = false,
                2 => game.view.as_mut().unwrap().can_act = false,
                3 => game.screen = None,
                4 => game.fatal = true,
                _ => {}
            }
            game.view_serial += 1;
        }
        if rejected == 5 {
            let generation = app.world().resource::<State>().generation;
            app.world_mut().spawn((
                Interaction::Pressed,
                Control {
                    generation: generation - 1,
                    action: super::Action::Confirm,
                },
                BackgroundColor::default(),
            ));
            app.update();
        } else {
            press(&mut app, Key::F7);
        }
        assert!(receiver.try_recv().is_err(), "rejection case {rejected}");
    }
}

#[test]
fn hover_exit_restores_the_card_color_without_erasing_keyboard_selection() {
    let (mut app, _) = app();
    let generation = app.world().resource::<State>().generation;
    let card = app
        .world_mut()
        .spawn((
            Interaction::Hovered,
            Control {
                generation,
                action: super::Action::Details("balances"),
            },
            BackgroundColor(chrome::BUTTON),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<BackgroundColor>(card).unwrap().0,
        chrome::HOT
    );
    app.world_mut().entity_mut(card).insert(Interaction::None);
    app.update();
    assert_eq!(
        app.world().get::<BackgroundColor>(card).unwrap().0,
        chrome::BUTTON
    );
    app.world_mut().entity_mut(card).insert(Row(0, generation));
    app.world_mut()
        .entity_mut(card)
        .insert(Interaction::Hovered);
    app.update();
    app.world_mut().entity_mut(card).insert(Interaction::None);
    app.update();
    assert_eq!(
        app.world().get::<BackgroundColor>(card).unwrap().0,
        chrome::HOT
    );
}

#[test]
fn keyboard_selection_scrolls_once_and_leaves_manual_scrolling_available() {
    let mut app = App::new();
    app.insert_resource(Game {
        screen: Some(Screen::Advancement),
        ..default()
    })
    .insert_resource(State {
        selected: Some(2),
        generation: 3,
        focus_pending: true,
        ..default()
    })
    .add_systems(Update, focus);
    let area = app
        .world_mut()
        .spawn((
            crate::character::PageScroll,
            ComputedNode {
                size: Vec2::new(300., 100.),
                content_size: Vec2::new(300., 400.),
                inverse_scale_factor: 1.,
                ..default()
            },
            GlobalTransform::from_translation(Vec3::new(200., 100., 0.)),
            ScrollPosition::default(),
        ))
        .id();
    app.world_mut().spawn((
        Row(2, 3),
        ComputedNode {
            size: Vec2::new(200., 40.),
            ..default()
        },
        GlobalTransform::from_translation(Vec3::new(200., 190., 0.)),
    ));
    app.update();
    assert_eq!(
        app.world().get::<ScrollPosition>(area).unwrap().offset_y,
        60.
    );
    app.world_mut()
        .get_mut::<ScrollPosition>(area)
        .unwrap()
        .offset_y = 0.;
    app.update();
    assert_eq!(
        app.world().get::<ScrollPosition>(area).unwrap().offset_y,
        0.,
        "unchanged selection must not fight manual scrolling"
    );
}

#[test]
fn confirmation_names_the_exact_retained_proposal_while_browsing_other_pages() {
    let (mut app, _) = app();
    preview(&mut app);
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        let page = game.advancement_view.page.as_mut().unwrap();
        page.query = AdvancementQuery::Ledger {
            basis: page.basis.clone(),
            offset: 0,
        };
        page.total = 0;
        page.entries.clear();
        page.validate().unwrap();
        game.advancement_serial += 1;
    }
    app.update();
    let world = app.world_mut();
    let button = world
        .query::<(Entity, &Control)>()
        .iter(world)
        .find_map(|(entity, control)| control.is_confirmation().then_some(entity))
        .unwrap();
    let label = world
        .query::<(&Text, &ChildOf)>()
        .iter(world)
        .find_map(|(text, parent)| (parent.parent() == button).then_some(text.0.as_str()))
        .unwrap();
    assert!(
        label.contains("Purchase HD 1 / pool practice / skill perception"),
        "the pending operation must remain explicit while browsing: {label}"
    );
}

#[test]
fn confirmation_stays_outside_the_scrolling_preview_body() {
    let (mut app, _) = app();
    preview(&mut app);
    app.update();
    let world = app.world_mut();
    let mut entity = world
        .query::<(Entity, &Control)>()
        .iter(world)
        .find_map(|(entity, control)| {
            matches!(control.action, super::Action::Confirm).then_some(entity)
        })
        .unwrap();
    while let Some(parent) = world.get::<ChildOf>(entity) {
        entity = parent.parent();
        assert!(
            world.get::<crate::character::PageScroll>(entity).is_none(),
            "Confirm must stay visible outside the scrolling preview body"
        );
    }
}

#[test]
fn typed_supply_pages_restore_visible_prose_and_remove_catalog_shortcuts() {
    let (mut app, mut receiver) = app();
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.advancement_view.supplies = true;
        game.advancement_view.lines = vec!["Ration: exact visible sources".into()];
        game.advancement = game.advancement_view.lines.clone();
        game.advancement_serial += 1;
    }
    app.update();
    let world = app.world_mut();
    let (text, node) = world
        .query::<(&crate::character::Field, &Text, &Node)>()
        .iter(world)
        .find_map(|(field, text, node)| {
            matches!(field, crate::character::Field::Page).then_some((text, node))
        })
        .unwrap();
    assert_eq!(text.0, "Ration: exact visible sources");
    assert_eq!(node.display, Display::Flex);
    assert!(world.resource::<State>().options.is_empty());
    press(&mut app, Key::Enter);
    assert!(
        receiver.try_recv().is_err(),
        "the hidden retained page must not handle Enter"
    );
}

#[test]
fn pressed_cards_and_footer_dispatch_their_exact_disclosed_choices_once() {
    fn click(app: &mut App, predicate: impl Fn(&Control) -> bool) {
        let world = app.world_mut();
        let entity = world
            .query::<(Entity, &Control)>()
            .iter(world)
            .find_map(|(entity, control)| predicate(control).then_some(entity))
            .unwrap();
        world.entity_mut(entity).insert(Interaction::Pressed);
        app.update();
    }
    let (mut app, mut receiver) = app();
    let shown = app
        .world()
        .resource::<Game>()
        .view
        .as_ref()
        .unwrap()
        .shown
        .clone();
    click(&mut app, |control| {
        matches!(
            &control.action,
            super::Action::Choose(AdvancementAction::Pool { .. })
        )
    });
    let sent = receiver.try_recv().unwrap();
    assert_eq!(sent.shown, shown);
    assert!(matches!(sent.command, Cmd::Advancement(text) if text == "skills"));
    assert!(receiver.try_recv().is_err());
    let (net, mut receiver) = crate::net::test_channel();
    app.insert_resource(net);
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.busy = false;
        let page = game.advancement_view.page.as_mut().unwrap();
        page.query = AdvancementQuery::Catalog {
            basis: page.basis.clone(),
            catalog: AdvancementKind::SkillRank,
            offset: 0,
        };
        page.entries = vec![AdvancementEntry::Catalog {
            value: AdvancementCatalogRow {
                selection: PurchaseChoice::SkillRank {
                    skill: "perception".into(),
                },
                name: "Perception".into(),
                form_choices: vec![],
            },
        }];
        page.validate().unwrap();
        game.advancement_serial += 1;
    }
    app.update();
    click(&mut app, |control| {
        matches!(
            &control.action,
            super::Action::Choose(AdvancementAction::Preview(_))
        )
    });
    let sent = receiver.try_recv().unwrap();
    assert_eq!(sent.shown, shown);
    assert!(
        matches!(sent.command, Cmd::Advancement(text) if text == "purchase 1 practice skill perception")
    );
    assert!(receiver.try_recv().is_err());
    assert!(app
        .world()
        .resource::<Game>()
        .advancement_view
        .confirmation
        .is_none());
    let (net, mut receiver) = crate::net::test_channel();
    app.insert_resource(net);
    app.world_mut().resource_mut::<Game>().busy = false;
    preview(&mut app);
    app.update();
    click(&mut app, Control::is_confirmation);
    let sent = receiver.try_recv().unwrap();
    assert_eq!(sent.shown, shown);
    assert!(matches!(sent.command, Cmd::Advancement(text) if text == "confirm"));
    assert!(
        receiver.try_recv().is_err(),
        "one explicit confirmation dispatches once"
    );
}

#[test]
fn character_upgrade_renders_result_cards_without_internal_build_prose() {
    let (mut app, _) = app();
    preview(&mut app);
    app.world_mut().resource_mut::<Game>().advancement =
        vec!["Resulting build 4; technical summary".into()];
    app.update();
    let world = app.world_mut();
    let values: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.as_str())
        .collect();
    assert!(
        values.contains(&"7 / 9"),
        "Health must have a distinct preview result card"
    );
    assert_eq!(
        values.iter().filter(|text| **text == "2 / 3").count(),
        3,
        "all three disclosed resource results are distinct cards"
    );
    let page = world
        .query::<(&crate::character::Field, &Node)>()
        .iter(world)
        .find_map(|(field, node)| matches!(field, crate::character::Field::Page).then_some(node))
        .unwrap();
    assert_eq!(
        page.display,
        Display::None,
        "the default preview must not show internal build bookkeeping"
    );
}

#[test]
fn preview_death_and_consequences_are_explicit_and_supply_mode_keeps_priority() {
    let (mut app, _) = app();
    preview(&mut app);
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        let confirmation = game.advancement_view.confirmation.as_mut().unwrap();
        confirmation.summary.health.current = 0;
        confirmation.summary.health.dead = true;
        confirmation.summary.health.injury = 12;
        confirmation.summary.removed_count = 3;
        confirmation.summary.refund_count = 2;
        confirmation.summary.removed_condition_count = 1;
        confirmation.summary.canceled_resource_holds = 4;
        confirmation.summary.strength_acquired = 2;
        confirmation.summary.health_recovery.stopped = true;
        let value = confirmation.summary.clone();
        game.advancement_view.page.as_mut().unwrap().entries =
            vec![AdvancementEntry::Summary { value }];
        game.advancement_view
            .page
            .as_ref()
            .unwrap()
            .validate()
            .unwrap();
    }
    app.update();
    let world = app.world_mut();
    let values: Vec<_> = world
        .query::<&Text>()
        .iter(world)
        .map(|text| text.0.as_str())
        .collect();
    for expected in ["THIS CHANGE WOULD LEAVE YOU DEAD", "0 / 9", "Injury: 12 · Permanent Strength acquired: 2", "Purchases removed: 3 · Refund entries: 2\nCondition entries removed: 1 · Resource holds released: 4", "Health recovery: 0 / 1200 active ticks · paused"] {
        assert!(values.contains(&expected), "missing disclosed consequence: {expected}");
    }
    let game = &mut *world.resource_mut::<Game>();
    game.advancement_view.supplies = true;
    game.advancement = vec!["Ration: exact supply chooser".into()];
    game.advancement_serial += 1;
    app.update();
    let world = app.world_mut();
    let (text, node) = world
        .query::<(&crate::character::Field, &Text, &Node)>()
        .iter(world)
        .find_map(|(field, text, node)| {
            matches!(field, crate::character::Field::Page).then_some((text, node))
        })
        .unwrap();
    assert_eq!(text.0, "Ration: exact supply chooser");
    assert_eq!(node.display, Display::Flex);
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "PREVIEW — NOT APPLIED"),
        "retained summary must not replace a supply chooser"
    );
}
