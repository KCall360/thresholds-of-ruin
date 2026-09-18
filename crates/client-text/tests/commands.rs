use tor_client_text::{parse, Input};
use tor_protocol::*;

#[test]
fn wizard_commands_are_structured_revision_checked_and_reject_unknown_settings() {
    assert_eq!(
        parse("wizard teleport 2 1 3 1 0", &state()).unwrap(),
        Input::Command(Command::Wizard {
            expected_revision: 7,
            operation: WizardOperation::Teleport {
                actor: ActorId(2),
                position: Position {
                    region: 1,
                    x: 3,
                    y: 1,
                    z: 0
                }
            }
        })
    );
    for command in [
        "wizard item sword 1 1 1 0",
        "wizard actor fast 1 1 1 0",
        "wizard rewind",
        "wizard teleport 1 2",
    ] {
        assert!(parse(command, &state()).is_err());
    }
    let mut marked = state();
    marked.wizard_game = true;
    assert!(tor_client_text::describe(&marked).contains("WIZARD GAME"));
}

#[test]
fn prose_renders_disclosed_positions_and_escapes_terminal_controls() {
    let mut state = state();
    state.observation.region.name = "Entry\u{1b}[2J".into();
    let prose = tor_client_text::describe(&state);
    assert!(!prose.contains('\u{1b}'));
    assert!(prose.contains("position (1, 1, 0); tick 100; revision 7"));
    assert!(prose.contains("copper token (#10)"));
    assert!(prose.contains("Within reach."));
    assert_eq!(tor_client_text::inventory(&state), "Inventory: empty.");
    assert_eq!(
        tor_client_text::safe("first\nsecond\t\u{009b}"),
        "first\\nsecond\\t\\u{9b}"
    );
}

#[test]
fn movement_queries_and_control_have_distinct_intents() {
    assert_eq!(
        parse("GO East", &state()).unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Move {
                direction: Direction::East
            }
        })
    );
    assert_eq!(parse("look", &state()).unwrap(), Input::Look);
    assert_eq!(
        parse("release", &state()).unwrap(),
        Input::Request(Request::ReleaseControl)
    );
    assert_eq!(
        parse("history abc", &state()).unwrap(),
        Input::Request(Request::History {
            before: Some(EntryId("abc".into())),
            limit: 50
        })
    );
    assert!(parse("history abc extra", &state()).is_err());
}

fn state() -> StateView {
    serde_json::from_value(serde_json::json!({
        "wizard_game":false,"revision": 7, "observation": {
            "actor": 1, "tick": 100, "position": {"region":1,"x":1,"y":1,"z":0},
            "region":{"id":1,"name":"Entry chamber","width":5,"depth":3,"height":1},
            "ground_items":[
                {"item":{"id":10,"name":"copper token"},"position":{"region":1,"x":1,"y":1,"z":0}},
                {"item":{"id":11,"name":"silver token"},"position":{"region":1,"x":2,"y":1,"z":0}}
            ], "inventory":[], "visible_actors":[], "exits":[], "known_places":[], "ready":true
        }
    }))
    .unwrap()
}

#[test]
fn resolves_only_disclosed_nouns_and_requires_clarification() {
    let state = state();
    assert!(parse("take token", &state).unwrap_err().contains("#10"));
    assert_eq!(
        parse("take the COPPER token", &state).unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Take { item: 10 }
        })
    );
    assert_eq!(
        parse("take #11", &state).unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Take { item: 11 }
        })
    );
    assert!(parse("take #999", &state).is_err());
    assert!(parse("take stone tablet", &state).is_err());
    assert!(parse("east extra", &state).is_err());
}

#[test]
fn annotations_preserve_text_and_explicit_scope_without_an_action() {
    assert_eq!(
        parse("note Remember the Gallery.", &state()).unwrap(),
        Input::Command(Command::Annotate {
            anchor: Anchor::State { revision: 7 },
            text: "Remember the Gallery.".into(),
            source: ClientSource::User,
            audience: Audience::Private,
            category: AnnotationCategory::Note
        })
    );
    assert_eq!(
        parse(
            "annotate frontend actor explanation entry:abc A useful hint",
            &state()
        )
        .unwrap(),
        Input::Command(Command::Annotate {
            anchor: Anchor::Entry {
                id: EntryId("abc".into())
            },
            text: "A useful hint".into(),
            source: ClientSource::Frontend,
            audience: Audience::Actor,
            category: AnnotationCategory::Explanation
        })
    );
    for bad in [
        "note   ",
        "note bad\u{1b}[2J",
        "annotate backend actor note here spoiler",
        "annotate user public note here text",
        "annotate user private note state:no text",
    ] {
        assert!(parse(bad, &state()).is_err(), "{bad}");
    }
    assert!(parse(&format!("note {}", "é".repeat(2049)), &state()).is_err());
}
