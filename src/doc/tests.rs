//! Undo grouping per REFERENCE-001 section 15, `version`, and line starts.
use std::time::Duration;

use super::{Change, Doc, Kind, Selection};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Types `text` at the caret at time `at` (ms).
fn type_at_caret(doc: &mut Doc, text: &str, at: u64) {
    let caret = doc.selection().head;
    let end = caret + text.len();
    doc.apply(
        vec![Change::insert(caret, text)],
        Selection::caret(end),
        Kind::Type,
        ms(at),
    );
}

/// Types a word one character every 100 ms from `start` (ms).
fn type_word(doc: &mut Doc, word: &str, start: u64) {
    for (i, c) in word.chars().enumerate() {
        type_at_caret(doc, &c.to_string(), start + 100 * i as u64);
    }
}

#[test]
fn a_typing_burst_is_one_undo_step_and_undo_restores_the_caret() {
    let mut doc = Doc::new("ab".into());
    doc.set_selection(Selection::caret(1));
    type_word(&mut doc, "xyz", 0);
    assert_eq!(doc.text(), "axyzb");
    assert!(doc.undo());
    assert_eq!((doc.text(), doc.selection()), ("ab", Selection::caret(1)));
    assert!(!doc.undo());
    assert!(doc.redo());
    assert_eq!(
        (doc.text(), doc.selection()),
        ("axyzb", Selection::caret(4))
    );
}

#[test]
fn a_pause_or_a_caret_move_starts_a_new_step() {
    let mut doc = Doc::new(String::new());
    type_word(&mut doc, "ab", 0);
    // 500 ms after the last change is not less than the burst delay.
    type_at_caret(&mut doc, "c", 100 + 500);
    doc.undo();
    assert_eq!(doc.text(), "ab");

    let mut doc = Doc::new(String::new());
    type_word(&mut doc, "ab", 0);
    doc.set_selection(Selection::caret(1));
    doc.set_selection(Selection::caret(2));
    type_at_caret(&mut doc, "c", 150);
    doc.undo();
    assert_eq!(doc.text(), "ab", "the move ended the burst");
}

#[test]
fn edits_apart_do_not_join_but_typing_then_deleting_does() {
    let mut doc = Doc::new("0123456789".into());
    doc.apply(
        vec![Change::insert(0, "a")],
        Selection::caret(1),
        Kind::Type,
        ms(0),
    );
    doc.apply(
        vec![Change::insert(9, "b")],
        Selection::caret(10),
        Kind::Type,
        ms(50),
    );
    doc.undo();
    assert_eq!(doc.text(), "a0123456789", "not next to the last change");

    let mut doc = Doc::new(String::new());
    type_word(&mut doc, "abc", 0);
    doc.apply(
        vec![Change::delete(2..3)],
        Selection::caret(2),
        Kind::Delete,
        ms(250),
    );
    assert_eq!(doc.text(), "ab");
    doc.undo();
    assert_eq!(doc.text(), "", "a backspace joins the burst it follows");
}

#[test]
fn a_paste_is_its_own_step() {
    let mut doc = Doc::new(String::new());
    type_word(&mut doc, "ab", 0);
    doc.apply(
        vec![Change::insert(2, "PASTED")],
        Selection::caret(8),
        Kind::Other,
        ms(150),
    );
    doc.undo();
    assert_eq!(doc.text(), "ab");
}

#[test]
fn version_comes_back_with_undo_and_is_new_after_an_edit() {
    let mut doc = Doc::new("x".into());
    let saved = doc.version();
    type_at_caret(&mut doc, "a", 0);
    let typed = doc.version();
    assert_ne!(typed, saved);
    type_at_caret(&mut doc, "b", 100);
    assert_ne!(doc.version(), typed, "every edit changes it, bursts too");
    doc.undo();
    assert_eq!(doc.version(), saved);
    doc.redo();
    let redone = doc.version();
    doc.undo();
    type_at_caret(&mut doc, "c", 2_000);
    assert!(![saved, typed, redone].contains(&doc.version()));
    assert!(!doc.redo(), "a new edit drops the redo steps");
}

#[test]
fn a_transaction_of_several_changes_undoes_at_once() {
    let mut doc = Doc::new("say bold here".into());
    doc.set_selection(Selection { anchor: 4, head: 8 });
    doc.apply(
        vec![Change::insert(4, "**"), Change::insert(8, "**")],
        Selection {
            anchor: 6,
            head: 10,
        },
        Kind::Other,
        ms(0),
    );
    assert_eq!(doc.text(), "say **bold** here");
    doc.undo();
    assert_eq!(doc.text(), "say bold here");
    assert_eq!(doc.selection().range(), 4..8);
}

#[test]
fn lines_end_at_lf_crlf_and_a_lone_cr() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/docs/line-endings.md"
    ))
    .unwrap();
    let doc = Doc::new(text.clone());
    assert_eq!(doc.text(), text, "loading keeps every byte");
    let lines: Vec<&str> = (0..doc.line_count())
        .map(|i| &doc.text()[doc.line_range(i)])
        .collect();
    assert_eq!(
        lines,
        [
            "# Mixed line endings",
            "CRLF line with trailing spaces   ",
            "\tTab-indented line",
            "Lone carriage return here",
            "next line after it",
            "**unclosed strong and `unclosed code",
            "[broken link](",
            "  \t  ",
            "Last line without a newline",
        ]
    );
    assert_eq!(doc.line_at(0), 0);
    assert_eq!(
        doc.line_at(doc.line_range(0).end + 1),
        0,
        "the \\n of a CRLF"
    );
    assert_eq!(doc.line_at(text.len()), 8);
}

#[test]
fn a_newline_typed_after_a_lone_cr_joins_it_into_one_ending() {
    let mut doc = Doc::new("a\rb".into());
    assert_eq!(doc.line_count(), 2);
    doc.apply(
        vec![Change::insert(2, "\n")],
        Selection::caret(3),
        Kind::Type,
        ms(0),
    );
    assert_eq!(doc.line_count(), 2, "\\r\\n is one ending");
    assert_eq!(doc.line_range(1), 3..4);
    doc.apply(
        vec![Change::delete(1..2)],
        Selection::caret(1),
        Kind::Delete,
        ms(5_000),
    );
    assert_eq!((doc.text(), doc.line_count()), ("a\nb", 2));
    doc.apply(
        vec![Change::insert(2, "\n")],
        Selection::caret(3),
        Kind::Type,
        ms(9_000),
    );
    assert_eq!(doc.line_count(), 3, "an empty line between");
    let text = "a\r";
    assert_eq!(Doc::new(text.into()).line_count(), 2, "an empty last line");
}
