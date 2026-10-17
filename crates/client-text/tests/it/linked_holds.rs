use tor_client_common::test_support;
use tor_client_text::{parse, Input};
use tor_protocol::*;

#[test]
fn escape_selects_a_single_hold_and_preserves_directional_sequence_escape() {
    let mut state = StateView {
        wizard_game: false,
        revision: 7,
        observation: test_support::observation(),
    };
    let hold = HoldTarget::from_digest([9; 32]);
    state.observation.linked_holds = vec![LinkedHold {
        target: hold,
        holder: None,
    }];
    for line in [
        "escape".to_owned(),
        "escape hold 1".into(),
        format!("escape hold #{hold}"),
        "escape hold unseen".into(),
    ] {
        assert_eq!(
            parse(&line, &state).unwrap(),
            Input::Command(Command::Act {
                expected_revision: 7,
                action: Action::Escape { hold }
            }),
            "{line}"
        );
    }
    assert_eq!(
        parse("escape west", &state).unwrap(),
        Input::CancelIntention {
            step: Some(Direction::West)
        }
    );
    let second = HoldTarget::from_digest([8; 32]);
    state.observation.linked_holds.push(LinkedHold {
        target: second,
        holder: None,
    });
    assert!(parse("escape", &state)
        .unwrap_err()
        .contains("Choose a hold"));
    assert!(parse("escape hold unseen", &state)
        .unwrap_err()
        .contains("more than one"));
    for line in ["escape hold", "escape nowhere", "escape from unseen"] {
        assert!(parse(line, &state).is_err(), "{line}");
    }
    assert!(tor_client_text::stats(&state).contains("2. Unseen holder"));

    let west = ActorTarget::from_digest([77; 32]);
    state.observation.visible_actors.push(ActorView {
        appearance_seed: None,
        id: west,
        name: "West".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
        asset: None,
    });
    state.observation.linked_holds[0].holder = Some(west);
    assert_eq!(
        parse("escape hold west", &state).unwrap(),
        Input::Command(Command::Act {
            expected_revision: 7,
            action: Action::Escape { hold }
        })
    );
    assert_eq!(
        parse("escape west", &state).unwrap(),
        Input::CancelIntention {
            step: Some(Direction::West)
        }
    );
    let other_west = ActorTarget::from_digest([78; 32]);
    let mut duplicate_name = state.observation.visible_actors.last().unwrap().clone();
    duplicate_name.id = other_west;
    state.observation.visible_actors.push(duplicate_name);
    state.observation.linked_holds[1].holder = Some(other_west);
    assert!(parse("escape hold west", &state)
        .unwrap_err()
        .contains("more than one"));
}
