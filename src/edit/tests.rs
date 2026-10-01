//! Motions and edits per REFERENCE-001 sections 13 and 18.
use std::time::Duration;

use super::{Motion, delete, enter, go, line_ending, paste, target, type_text};
use crate::doc::{Doc, Selection};

fn doc_at(text: &str, caret: usize) -> Doc {
    let mut doc = Doc::new(text.into());
    doc.set_selection(Selection::caret(caret));
    doc
}

#[test]
fn left_and_right_step_over_graphemes_and_crlf() {
    let doc = doc_at("e\u{301}x\r\ny😀", 0);
    let rights: Vec<usize> = std::iter::successors(Some(0), |&at| {
        let next = target(&doc, at, Motion::Right);
        (next != at).then_some(next)
    })
    .collect();
    assert_eq!(rights, [0, 3, 4, 6, 7, 11]);
    assert_eq!(target(&doc, 6, Motion::Left), 4, "back over \\r\\n at once");
    assert_eq!(target(&doc, 0, Motion::Left), 0);
}

#[test]
fn word_motions_skip_spaces_then_one_run() {
    let doc = doc_at("foo_bar, **baz**\nnext", 0);
    let mut at = 0;
    let mut stops = Vec::new();
    while at < doc.text().len() {
        at = target(&doc, at, Motion::WordRight);
        stops.push(at);
    }
    assert_eq!(stops, [7, 8, 11, 14, 16, 17, 21]);
    assert_eq!(target(&doc, 14, Motion::WordLeft), 11);
    assert_eq!(
        target(&doc, 17, Motion::WordLeft),
        16,
        "a line ending alone"
    );
    assert_eq!(target(&doc, 11, Motion::WordLeft), 9);
}

#[test]
fn plain_left_and_right_collapse_a_selection() {
    let mut doc = doc_at("abcdef", 0);
    doc.set_selection(Selection { anchor: 4, head: 2 });
    go(&mut doc, Motion::Right, false);
    assert_eq!(doc.selection(), Selection::caret(4));
    go(&mut doc, Motion::LineStart, true);
    assert_eq!(doc.selection(), Selection { anchor: 4, head: 0 });
    go(&mut doc, Motion::Left, false);
    assert_eq!(doc.selection(), Selection::caret(0));
}

#[test]
fn enter_and_paste_use_the_documents_line_ending() {
    assert_eq!(line_ending("a\r\nb\n"), "\r\n");
    assert_eq!(line_ending("a\rb"), "\r");
    assert_eq!(line_ending("one line"), "\n");
    let mut doc = doc_at("a\r\nb", 1);
    enter(&mut doc, Duration::ZERO);
    assert_eq!(doc.text(), "a\r\n\r\nb");
    paste(&mut doc, "x\ny\rz\r\n", Duration::ZERO);
    assert_eq!(doc.text(), "a\r\nx\r\ny\r\nz\r\n\r\nb");
}

#[test]
fn deleting_takes_the_selection_or_one_motion() {
    let mut doc = doc_at("ab\r\ncd", 4);
    delete(&mut doc, Motion::Left, Duration::ZERO);
    assert_eq!((doc.text(), doc.selection()), ("abcd", Selection::caret(2)));
    delete(&mut doc, Motion::Right, Duration::ZERO);
    assert_eq!(doc.text(), "abd");
    doc.set_selection(Selection { anchor: 3, head: 0 });
    type_text(&mut doc, "x", Duration::ZERO);
    assert_eq!((doc.text(), doc.selection()), ("x", Selection::caret(1)));
    delete(&mut doc, Motion::Right, Duration::ZERO);
    assert_eq!(doc.text(), "x", "nothing after the caret");
}
