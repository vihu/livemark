//! Find and replace per REFERENCE-001 section 16.
use std::time::Duration;

use super::{Query, find, next_match, prev_match, replace, replace_all};
use crate::doc::{Doc, Selection};

fn query(text: &str) -> Query {
    Query {
        text: text.into(),
        case_sensitive: false,
    }
}

fn selected(doc: &Doc) -> &str {
    &doc.text()[doc.selection().range()]
}

#[test]
fn matches_ignore_case_by_default_and_never_cut_characters() {
    let text = "Note, NOTE and note; Ärger ärger";
    let found = |q: Query| -> Vec<&str> { q.matches(text).into_iter().map(|r| &text[r]).collect() };
    assert_eq!(found(query("note")), ["Note", "NOTE", "note"]);
    assert_eq!(found(query("ÄRGER")), ["Ärger", "ärger"], "beyond ASCII");
    let exact = Query {
        text: "note".into(),
        case_sensitive: true,
    };
    assert_eq!(found(exact), ["note"]);
    assert!(query("").matches(text).is_empty());
    assert_eq!(query("aa").matches("aaaa"), [0..2, 2..4], "not overlapping");
}

#[test]
fn next_and_previous_wrap_around() {
    let matches = [2..4, 6..8];
    assert_eq!(next_match(&matches, 0, 0), Some(2..4));
    assert_eq!(next_match(&matches, 2, 4), Some(6..8));
    assert_eq!(next_match(&matches, 6, 8), Some(2..4), "wraps to the top");
    assert_eq!(prev_match(&matches, 6, 8), Some(2..4));
    assert_eq!(prev_match(&matches, 2, 4), Some(6..8), "wraps to the end");
    assert_eq!(
        next_match(std::slice::from_ref(&(2..4)), 2, 4),
        None,
        "the only match, already selected"
    );
    let mut doc = Doc::new("a cat, a Cat".into());
    assert!(find(&mut doc, &query("cat"), true));
    assert_eq!(doc.selection(), Selection { anchor: 2, head: 5 });
    assert!(find(&mut doc, &query("cat"), true));
    assert_eq!(selected(&doc), "Cat");
    assert!(!find(&mut doc, &query("dog"), true));
}

#[test]
fn replace_changes_the_selected_match_then_selects_the_next() {
    let mut doc = Doc::new("one two one two one".into());
    let q = query("one");
    // Nothing selected: the first press only finds.
    replace(&mut doc, &q, "1", Duration::ZERO);
    assert_eq!(
        (doc.text(), doc.selection().range()),
        ("one two one two one", 0..3)
    );
    replace(&mut doc, &q, "1", Duration::ZERO);
    assert_eq!(doc.text(), "1 two one two one");
    assert_eq!(selected(&doc), "one");
    assert_eq!(doc.selection().range(), 6..9, "in the new text");
}

#[test]
fn replace_all_is_one_undo_step() {
    let mut doc = Doc::new("a-b-c".into());
    doc.set_selection(Selection::caret(5));
    assert_eq!(replace_all(&mut doc, &query("-"), " + ", Duration::ZERO), 2);
    assert_eq!(doc.text(), "a + b + c");
    assert_eq!(
        doc.selection(),
        Selection::caret(9),
        "the caret kept its place"
    );
    doc.undo();
    assert_eq!(doc.text(), "a-b-c");
}
