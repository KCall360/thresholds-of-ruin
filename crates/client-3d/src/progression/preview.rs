//! Player-facing preview results; internal build revisions remain diagnostic data.
use super::*;
use tor_protocol::{AdvancementPreviewSection, AdvancementSummary};

pub(super) fn visible(game: &Game) -> bool {
    if game.advancement_view.supplies {
        return false;
    }
    let Some(view) = game.view.as_ref().filter(|view| view.has_control) else {
        return false;
    };
    let Some(page) = game.advancement_view.current_page(&view.shown) else {
        return false;
    };
    let Some(confirmation) = &game.advancement_view.confirmation else {
        return false;
    };
    matches!(&page.query, AdvancementQuery::Preview { proposal, section: AdvancementPreviewSection::Summary, .. }
        if proposal == &confirmation.proposal && confirmation.summary.proposal == confirmation.proposal && confirmation.basis == page.basis && confirmation.revision == page.revision)
}
fn stat(parent: &mut ChildSpawnerCommands, label: &str, value: String, detail: Option<String>) {
    parent
        .spawn((
            BackgroundColor(chrome::BUTTON),
            BorderColor(chrome::GOLD),
            Node {
                width: Val::Percent(48.),
                flex_grow: 1.,
                padding: UiRect::all(Val::Px(10.)),
                border: UiRect::all(Val::Px(1.)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.),
                ..default()
            },
        ))
        .with_children(|card| {
            text(card, label.to_uppercase(), 15.);
            text(card, value, 23.);
            if let Some(detail) = detail {
                text(card, detail, 15.);
            }
        });
}
pub(super) fn draw(root: &mut ChildSpawnerCommands, game: &Game, summary: &AdvancementSummary) {
    text(root, "PREVIEW — NOT APPLIED", 23.);
    if summary.health.dead {
        root.spawn((
            Text::new("THIS CHANGE WOULD LEAVE YOU DEAD"),
            chrome::text(19., Color::srgb(1., 0.35, 0.25)),
            Node {
                width: Val::Percent(100.),
                flex_shrink: 0.,
                ..default()
            },
        ));
    }
    root.spawn(Node {
        width: Val::Percent(100.),
        flex_wrap: FlexWrap::Wrap,
        row_gap: Val::Px(8.),
        column_gap: Val::Px(8.),
        flex_shrink: 0.,
        ..default()
    })
    .with_children(|grid| {
        let current = game.view.as_ref().and_then(|view| view.hp).map_or_else(
            || "Current health not disclosed".into(),
            |(hp, max)| format!("Current: {hp} / {max}"),
        );
        stat(
            grid,
            "Health",
            format!("{} / {}", summary.health.current, summary.health.maximum),
            Some(current),
        );
        for resource in [
            &summary.resources.stamina,
            &summary.resources.focus,
            &summary.resources.mana,
        ] {
            stat(
                grid,
                &resource.resource.name,
                format!("{} / {}", resource.balance, resource.maximum),
                None,
            );
        }
    });
    text(
        root,
        format!(
            "Injury: {} · Permanent Strength acquired: {}",
            summary.health.injury, summary.strength_acquired
        ),
        16.,
    );
    text(root, format!("Purchases removed: {} · Refund entries: {}\nCondition entries removed: {} · Resource holds released: {}", summary.removed_count, summary.refund_count, summary.removed_condition_count, summary.canceled_resource_holds), 16.);
    text(
        root,
        format!(
            "Health recovery: {} / {} active ticks{}",
            summary.health_recovery.elapsed_ticks,
            summary.health_recovery.period_ticks,
            if summary.health_recovery.stopped {
                " · paused"
            } else {
                ""
            }
        ),
        16.,
    );
}
