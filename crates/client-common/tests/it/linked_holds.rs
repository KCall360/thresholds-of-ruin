use tor_client_common::{combat_diagnostics, narration, presentation, test_support, Palette};
use tor_protocol::*;

#[test]
fn flight_and_grasp_creatures_have_explicit_disclosed_nouns_and_glyphs() {
    let cases = [
        ("creature.bat", 'B', "bat"),
        ("creature.bird.kestrel", 'K', "kestrel"),
        ("creature.plant.venus.flytrap", 'F', "venus flytrap"),
        ("creature.griffin", 'G', "griffin"),
    ];
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: cases
                .iter()
                .map(|(asset, _, _)| (*asset).to_owned())
                .collect(),
        },
    });
    for (asset, glyph, noun) in cases {
        let shown = presentation::resolve(&palette, Some(asset)).unwrap();
        assert_eq!(shown.ascii.glyph, glyph);
        assert_eq!(shown.noun.singular, noun);
        assert_eq!(shown.noun.message_key, format!("presentation.{asset}.noun"));
        assert!(presentation::resolve(&Palette::default(), Some(asset)).is_none());
    }
}

#[test]
fn linked_hold_public_narration_and_private_diagnostics_remain_separate() {
    let mut view = test_support::observation();
    view.visible_actors.clear();
    let hold = HoldTarget::from_digest([9; 32]);
    assert_eq!(
        narration::action(&Event::EscapeStarted { hold }, &view),
        "You begin trying to escape a hold."
    );
    for (success, expected) in [
        (false, "You fail to escape the hold."),
        (true, "You escape one hold."),
    ] {
        assert_eq!(
            narration::combat_event(
                &CombatEventView::EscapeResolved { hold, success },
                &view,
                &view
            ),
            expected
        );
    }
    let combatant = DiagnosticCombatant {
        health: 10,
        maximum_health: 10,
        injury: Some(0),
        dead: false,
        resources: vec![],
        conditions: vec![],
    };
    let report = CombatDiagnosticsView {
        enabled: true,
        tick: 10,
        captured: 1,
        dropped: 0,
        retained: 1,
        through: 1,
        records: vec![CombatDiagnosticRecord {
            sequence: 1,
            tick: 10,
            actor: ActorId(1),
            target: ActorId(2),
            ability: None,
            intention: None,
            origin_intention: None,
            preparation: 5,
            started: 5,
            remaining: 5,
            recovery: 2,
            charge: None,
            actor_before: combatant.clone(),
            target_before: combatant.clone(),
            actor_after: combatant.clone(),
            target_after: combatant,
            outcome: DiagnosticOutcome {
                result: AbilityOutcome::Applied,
                effects: vec![DiagnosticEffect::LinkedHold {
                    target: ActorId(2),
                    holder: ActorId(1),
                }],
            },
            trace: CombatTraceView {
                steps: vec![],
                truncated: false,
            },
        }],
    };
    assert!(combat_diagnostics::lines(&report)
        .unwrap()
        .iter()
        .any(|line| line == "  Actor 2: linked hold from actor 1"));
}
