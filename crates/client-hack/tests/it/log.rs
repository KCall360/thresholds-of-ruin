use tor_client_hack::{MessageLog, MORE, SCROLLBACK_CAP, SKIPPED, UNACKED_CAP};

#[test]
fn wrapped_row_caps_drop_the_oldest_unacked_and_scrollback() {
    let mut log = MessageLog::default();
    for index in 0..40 {
        log.append(&format!("row {index}"));
    }
    assert_eq!(log.pending().len(), UNACKED_CAP);
    assert_eq!(log.pending()[0], SKIPPED);
    assert_eq!(log.pending()[1], "row 9");
    assert_eq!(log.pending()[31], "row 39");

    let lines: Vec<String> = (0..40).map(|index| format!("row {index}")).collect();
    let mut batched = MessageLog::default();
    batched.append_narration(lines.iter().map(String::as_str));
    assert_eq!(batched.pending(), log.pending());

    let mut wide = MessageLog::default();
    wide.append(&"x".repeat(75 * 40));
    assert_eq!(wide.pending().len(), UNACKED_CAP);
    assert_eq!(wide.pending()[0], SKIPPED);
    assert_eq!(wide.pending()[1].chars().count(), 75);

    let mut paged = MessageLog::default();
    for index in 0..300 {
        paged.append(&format!("s{index}"));
        while paged.more() {
            assert!(paged.acknowledge());
        }
    }
    assert_eq!(paged.scrollback().len(), SCROLLBACK_CAP);
    assert_eq!(paged.scrollback().first().map(String::as_str), Some("s42"));
    assert_eq!(paged.scrollback().last().map(String::as_str), Some("s297"));
    assert_eq!(paged.pending(), &["s298".to_owned(), "s299".to_owned()][..]);
    assert!(!paged.more());
}

#[test]
fn snapshot_and_branch_change_leave_the_log_empty() {
    let mut log = MessageLog::default();
    log.append_narration(["kept", "second", "third", "fourth"]);
    assert!(log.acknowledge());
    log.open_scrollback();
    log.clear_for_snapshot();
    assert!(log.pending().is_empty());
    assert!(log.scrollback().is_empty());
    assert!(!log.scrollback_open());
    assert!(!log.more());

    log.append("again");
    log.open_scrollback();
    log.clear_for_branch_change();
    assert!(log.pending().is_empty());
    assert!(log.scrollback().is_empty());
    assert!(!log.scrollback_open());
}

#[test]
fn four_line_narration_sets_more_and_keeps_the_fourth_line() {
    let mut log = MessageLog::default();
    log.append_narration(["one", "two", "three", "four"]);
    assert!(log.more());
    assert_eq!(
        log.pending(),
        &[
            "one".to_owned(),
            "two".into(),
            "three".into(),
            "four".into()
        ][..]
    );
    assert_eq!(
        log.display(),
        vec!["one".to_owned(), "two".into(), MORE.to_owned()]
    );
    assert!(log.acknowledge());
    assert_eq!(log.pending(), &["three".to_owned(), "four".into()][..]);
    assert!(!log.more());

    let mut short = MessageLog::default();
    short.append_narration(["a", "b", "c"]);
    assert!(!short.more());
    assert!(!short.acknowledge());
    assert_eq!(short.pending().len(), 3);
    assert_eq!(short.display().len(), 3);
}

#[test]
fn a_line_longer_than_seventy_five_columns_occupies_more_than_one_row() {
    let mut log = MessageLog::default();
    log.append(&format!("{}{}", "y".repeat(75), "Z"));
    assert!(log.pending().len() > 1);
    assert_eq!(log.pending()[0], "y".repeat(75));
    assert_eq!(log.pending()[1], "Z");
    log.append("A\u{1}B");
    assert_eq!(log.pending()[2], "A B");
}
