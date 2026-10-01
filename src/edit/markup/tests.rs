//! Enter, Backspace and Tab around markup, per REFERENCE-001 section 7.
//! `|` marks the caret in each case.
use std::time::Duration;

use super::{continue_markup, delete_markup, indent, soft_break};
use crate::doc::{Doc, Selection};

/// A document from `text` with `|` as the caret.
fn doc(text: &str) -> Doc {
    let caret = text.find('|').expect("a caret");
    let mut doc = Doc::new(text.replacen('|', "", 1));
    doc.set_selection(Selection::caret(caret));
    doc
}

/// The text with `|` at the caret.
fn shown(doc: &Doc) -> String {
    let mut text = doc.text().to_owned();
    text.insert(doc.selection().head, '|');
    text
}

fn enter(text: &str) -> Option<String> {
    let mut doc = doc(text);
    continue_markup(&mut doc, Duration::ZERO).then(|| shown(&doc))
}

fn backspace(text: &str) -> Option<String> {
    let mut doc = doc(text);
    delete_markup(&mut doc, Duration::ZERO).then(|| shown(&doc))
}

fn tab(text: &str, outdent: bool) -> String {
    let mut doc = doc(text);
    indent(&mut doc, outdent, Duration::ZERO);
    shown(&doc)
}

#[test]
fn enter_continues_lists_and_quotes() {
    assert_eq!(enter("- a|").as_deref(), Some("- a\n- |"));
    assert_eq!(
        enter("* a  |").as_deref(),
        Some("* a\n* |"),
        "trailing spaces go"
    );
    assert_eq!(
        enter("1. a|\n2. b").as_deref(),
        Some("1. a\n2. |\n3. b"),
        "renumbered"
    );
    assert_eq!(enter("1) a|").as_deref(), Some("1) a\n2) |"));
    assert_eq!(
        enter("- [x] done|").as_deref(),
        Some("- [x] done\n- [ ] |"),
        "unchecked"
    );
    assert_eq!(enter("> q|").as_deref(), Some("> q\n> |"));
    assert_eq!(
        enter("> - a|").as_deref(),
        Some("> - a\n> - |"),
        "both levels"
    );
    assert_eq!(
        enter("- a\r\n- b|").as_deref(),
        Some("- a\r\n- b\r\n- |"),
        "the document's ending"
    );
    assert_eq!(enter("plain|"), None);
}

#[test]
fn enter_on_an_empty_item_takes_away_one_level() {
    assert_eq!(
        enter("- a\n- |").as_deref(),
        Some("- a\n|"),
        "out of the list"
    );
    // An empty item cannot interrupt a paragraph (CommonMark 5.2), so the
    // nested one needs a sibling before it to be an item at all.
    assert_eq!(
        enter("- a\n  - b\n  - |").as_deref(),
        Some("- a\n  - b\n- |"),
        "out to the parent"
    );
    assert_eq!(
        enter("1. a\n2. |\n3. b").as_deref(),
        Some("1. a\n|\n2. b"),
        "renumbered down"
    );
}

#[test]
fn a_second_empty_quoted_line_ends_the_quote() {
    assert_eq!(enter("> q\n> |").as_deref(), Some("> q\n>\n> |"));
    assert_eq!(enter("> q\n>\n> |").as_deref(), Some("> q\n\n|"));
}

#[test]
fn enter_in_fenced_code_keeps_the_indentation() {
    assert_eq!(
        enter("```\n  x|\n```").as_deref(),
        Some("```\n  x\n  |\n```")
    );
    assert_eq!(
        enter("```\n- x|\n```").as_deref(),
        Some("```\n- x\n|\n```"),
        "no markup in code"
    );
}

#[test]
fn shift_enter_breaks_to_the_items_text() {
    let mut doc = doc("- a|");
    soft_break(&mut doc, Duration::ZERO);
    assert_eq!(shown(&doc), "- a\n  |");
}

#[test]
fn backspace_after_markup_takes_it_away_a_step_at_a_time() {
    assert_eq!(
        backspace("-   |").as_deref(),
        Some("- |"),
        "extra spaces first"
    );
    assert_eq!(
        backspace("- |").as_deref(),
        Some("|"),
        "a first item's marker"
    );
    assert_eq!(
        backspace("- a\n- |").as_deref(),
        Some("- a\n  |"),
        "a later item continues the one before"
    );
    assert_eq!(backspace("> |").as_deref(), Some("|"));
    assert_eq!(backspace("- a|"), None, "not after markup");
}

#[test]
fn tab_moves_an_item_and_its_children_under_the_one_before() {
    assert_eq!(tab("- a\n- b|", false), "- a\n  - b|");
    assert_eq!(tab("- a|", false), "- a|", "a first item stays");
    assert_eq!(tab("- a\n- b|\n  - c", false), "- a\n  - b|\n    - c");
    assert_eq!(tab("1. a\n2. b|\n3. c", false), "1. a\n   1. b|\n2. c");
    assert_eq!(tab("- a\n  - b|", true), "- a\n- b|");
    assert_eq!(tab("x|", false), "x\t|", "outside a list, a tab");
    assert_eq!(tab("    x|", true), "x|");
    assert_eq!(tab("\tx|", true), "x|");
}
