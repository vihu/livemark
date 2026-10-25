//! Enter, Backspace and Tab around markup, per REFERENCE-001 section 7.
//! `|` marks the caret in each case.
use std::time::Duration;

use super::indent_tests::tab;
use super::{continue_markup, delete_markup, soft_break};
use crate::doc::{Doc, Selection};

/// A document from `text` with `|` as the caret.
pub(super) fn doc(text: &str) -> Doc {
    let caret = text.find('|').expect("a caret");
    let mut doc = Doc::new(text.replacen('|', "", 1));
    doc.set_selection(Selection::caret(caret));
    doc
}

/// The text with `|` at the caret.
pub(super) fn shown(doc: &Doc) -> String {
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
        Some("- a\n\n|"),
        "out of the list, a blank line between"
    );
    assert_eq!(
        enter("> - a\n> - |").as_deref(),
        Some("> - a\n>\n> |"),
        "in a quote too"
    );
    assert_eq!(
        enter("- a\n\n- |").as_deref(),
        Some("- a\n\n|"),
        "already apart"
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
    assert_eq!(
        enter("> ```\n> code|\n> ```").as_deref(),
        Some("> ```\n> code\n> |\n> ```"),
        "in a quote, its marker"
    );
    assert_eq!(
        enter("```\n> x|\n```").as_deref(),
        Some("```\n> x\n|\n```"),
        "a `>` in code is code"
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
fn tab_indented_lists_continue_and_end_with_their_tabs() {
    assert_eq!(enter("- a\n\t- b|").as_deref(), Some("- a\n\t- b\n\t- |"));
    assert_eq!(
        enter("- a\n\t- b\n\t- |").as_deref(),
        Some("- a\n\t- b\n- |"),
        "an empty nested item moves out"
    );
    assert_eq!(
        enter("1. a\n\t1. b|").as_deref(),
        Some("1. a\n\t1. b\n\t2. |")
    );
    assert_eq!(enter("- a\n\t> q|").as_deref(), Some("- a\n\t> q\n\t> |"));
    let deleted = backspace("- a\n\t- b\n\t- |").expect("markup taken");
    assert!(!deleted.contains("\t-|"), "not one space: {deleted:?}");
}

#[test]
fn shift_enter_in_fenced_code_in_a_list_item_keeps_the_indentation() {
    let mut doc = doc("- ```\n  code|\n  ```\n");
    soft_break(&mut doc, Duration::ZERO);
    assert_eq!(shown(&doc), "- ```\n  code\n  |\n  ```\n");
}

#[test]
fn three_levels_of_tabs_continue_as_siblings() {
    assert_eq!(
        enter("- a\n\t- b\n\t\t- c|").as_deref(),
        Some("- a\n\t- b\n\t\t- c\n\t\t- |")
    );
    assert_eq!(
        enter("> - a\n> \t- b\n> \t\t- c|").as_deref(),
        Some("> - a\n> \t- b\n> \t\t- c\n> \t\t- |")
    );
    let mut doc = doc("- a\n\t- b\n\t\t- c\n\t\t- |");
    soft_break(&mut doc, Duration::ZERO);
    assert_eq!(shown(&doc), "- a\n\t- b\n\t\t- c\n\t\t- \n\t\t  |");
    // Backspace after the first nested item's marker takes the markup.
    assert_eq!(backspace("- a\n\n\t- |b").as_deref(), Some("- a\n\n\t|b"));
}

#[test]
fn shift_enter_over_a_selection_in_fenced_code_breaks_the_line() {
    let mut doc = Doc::new("```\nabcdef\n```\n".into());
    doc.set_selection(Selection { anchor: 6, head: 8 });
    soft_break(&mut doc, Duration::ZERO);
    assert_eq!(shown(&doc), "```\nab\n|ef\n```\n");
}

#[test]
fn a_new_line_under_an_item_on_its_parents_line_blanks_the_parents_marker() {
    let mut quoted = doc("- a\n- > q|\n- c");
    soft_break(&mut quoted, Duration::ZERO);
    assert_eq!(shown(&quoted), "- a\n- > q\n  > |\n- c");
    assert_eq!(
        enter("1. a\n2. - b|\n3. c").as_deref(),
        Some("1. a\n2. - b\n   - |\n3. c")
    );
    let mut nested = doc("1. a\n2. - b|");
    soft_break(&mut nested, Duration::ZERO);
    assert_eq!(shown(&nested), "1. a\n2. - b\n     |");
}

#[test]
fn enter_moves_the_children_of_an_item_whose_number_gains_a_digit() {
    assert_eq!(
        enter("8. a|\n9. b\n   - c\n\n   text").as_deref(),
        Some("8. a\n9. |\n10. b\n    - c\n\n    text")
    );
    assert_eq!(
        enter("> 8. a|\n> 9. b\n>    - c").as_deref(),
        Some("> 8. a\n> 9. |\n> 10. b\n>     - c")
    );
}

#[test]
fn enter_in_fenced_code_with_the_caret_in_the_prefix_opens_a_line_above() {
    for text in ["> ```\n> a\n|> b\n> ```", "> ```\n> a\n>| b\n> ```"] {
        assert_eq!(enter(text).as_deref(), Some("> ```\n> a\n>\n> |b\n> ```"));
    }
    assert_eq!(
        enter("- ```\n|  x\n  ```").as_deref(),
        Some("- ```\n\n  |x\n  ```")
    );
}

#[test]
fn a_tab_after_a_list_marker_is_its_space() {
    assert_eq!(enter("-\tw1|").as_deref(), Some("-\tw1\n-\t|"));
    assert_eq!(enter("1.\tw1|").as_deref(), Some("1.\tw1\n2.\t|"));
    assert_eq!(tab("-\ta\n-\tb|", false), "-\ta\n    -\tb|");
    assert_eq!(tab("|-\ta", false), "|-\ta", "a first item stays");
}

#[test]
fn enter_on_an_empty_item_on_its_parents_line_keeps_the_parent() {
    assert_eq!(enter("1. - |\n2. c").as_deref(), Some("1. |\n2. c"));
}

#[test]
fn backspace_after_a_bare_quote_marker_and_a_quote_in_an_item() {
    assert_eq!(
        backspace(">- a\n>\n>- |b").as_deref(),
        Some(">- a\n>\n>   |b")
    );
    assert_eq!(backspace("- > |q").as_deref(), Some("- |q"));
}

#[test]
fn a_new_line_after_a_lazy_quote_line_takes_the_quotes_markers() {
    assert_eq!(
        enter("> > nested\nlazy line|").as_deref(),
        Some("> > nested\nlazy line\n> > |")
    );
    let mut lazy = doc("> > nested\nlazy line|");
    soft_break(&mut lazy, Duration::ZERO);
    assert_eq!(shown(&lazy), "> > nested\nlazy line\n> > |");
}
