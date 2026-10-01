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

#[test]
fn home_goes_to_the_row_start_then_toggles_indentation_and_line_start() {
    use super::line_boundary;
    // One source line, wrapped as two rows: 0..10 and 10..20.
    let doc = doc_at("    aaaa bbbbbbbbbbb", 0);
    let home = |head, row: std::ops::Range<usize>| line_boundary(&doc, head, row, false, None);
    assert_eq!(home(15, 10..20), 10, "the row's start first");
    assert_eq!(home(10, 10..20), 4, "then the end of the indentation");
    assert_eq!(home(4, 0..10), 0, "then the line start");
    assert_eq!(home(0, 0..10), 4, "and back");
    let end = |head, row: std::ops::Range<usize>| line_boundary(&doc, head, row, true, None);
    assert_eq!(end(2, 0..10), 10);
    assert_eq!(end(10, 0..10), 20, "from the row's end, the line's end");
}

#[test]
fn a_double_click_takes_the_class_on_the_clicked_side() {
    use super::word_at;
    let doc = doc_at("say **bold_er**,  ok\n\nx", 0);
    let word = |at, before| &doc.text()[word_at(&doc, at, before)];
    assert_eq!(word(7, false), "bold_er");
    assert_eq!(word(6, true), "**", "right half of the second *");
    assert_eq!(word(6, false), "bold_er");
    assert_eq!(word(16, false), "  ");
    assert_eq!(word(20, true), "ok", "at the line's end, the word before");
    assert_eq!(word(21, false), "", "an empty line");
    assert_eq!(word(0, true), "say", "at the line's start, the word after");
}

#[test]
fn copy_with_nothing_selected_takes_the_line_and_pastes_it_above() {
    use super::{copied, line_with_ending, paste_line};
    let mut doc = doc_at("one\r\ntwo\r\nthree", 6);
    assert_eq!(line_with_ending(&doc, 6), 5..10);
    assert_eq!(
        line_with_ending(&doc, 12),
        10..15,
        "the last line has no ending"
    );
    let (text, range, linewise) = copied(&doc);
    assert_eq!((text.as_str(), range, linewise), ("two", 5..10, true));
    paste_line(&mut doc, &text, Duration::ZERO);
    assert_eq!(doc.text(), "one\r\ntwo\r\ntwo\r\nthree");
    assert_eq!(
        doc.selection(),
        Selection::caret(11),
        "the caret moved with its line"
    );
    doc.set_selection(Selection { anchor: 0, head: 3 });
    assert_eq!(copied(&doc), ("one".into(), 0..3, false));
}

#[test]
fn home_stops_after_list_and_quote_markup_first() {
    use super::line_boundary;
    let doc = doc_at("  - [ ] task text", 0);
    let home = |head| line_boundary(&doc, head, 0..17, false, Some(8));
    assert_eq!(home(17), 8, "the item's text, not the line's start");
    assert_eq!(home(8), 0, "then the line's start");
    assert_eq!(home(0), 8, "and back");
}
