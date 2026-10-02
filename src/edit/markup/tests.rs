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

/// Tab or Shift+Tab over the selection `[` to `]` in `text`.
fn tab_over(text: &str, outdent: bool) -> String {
    let (anchor, head) = (text.find('[').unwrap(), text.find(']').unwrap() - 1);
    let mut doc = Doc::new(text.replacen('[', "", 1).replacen(']', "", 1));
    doc.set_selection(Selection { anchor, head });
    indent(&mut doc, outdent, Duration::ZERO);
    let range = doc.selection().range();
    let mut shown = doc.text().to_owned();
    shown.insert(range.end, ']');
    shown.insert(range.start, '[');
    shown
}

#[test]
fn tab_moves_every_item_a_selection_reaches() {
    assert_eq!(
        tab_over("- a\n- [b\n- c]\n- d", false),
        "- a\n  - [b\n  - c]\n- d"
    );
    assert_eq!(
        tab_over("- a\n  - [b\n  - c]\n- d", true),
        "- a\n- [b\n- c]\n- d"
    );
    assert_eq!(
        tab_over("- a\n- [b\n]- c", false),
        "- a\n  - [b\n]- c",
        "a selection ending at a line's start leaves that line"
    );
    assert_eq!(
        tab_over("1. a\n2. [b\n3. c]\n4. d\n5. e", false),
        "1. a\n   1. [b\n   2. c]\n2. d\n3. e"
    );
}

#[test]
fn a_selection_from_a_lines_start_moves_the_item_on_that_line() {
    assert_eq!(
        tab_over("- z\n- a\n[  - b\n  - c]", false),
        "- z\n- a\n[  - b\n  - c]",
        "b is a first item: nothing moves, not a"
    );
    assert_eq!(
        tab_over("- z\n- a\n  - y\n[  - b\n  - c\n]", false),
        "- z\n- a\n  - y\n[    - b\n    - c\n]"
    );
    assert_eq!(tab_over("- a\n[  - b\n  - c\n]", true), "- a\n[- b\n- c\n]");
}

#[test]
fn renumbering_replaces_the_digits_as_written() {
    assert_eq!(tab("1. a\n02. b|", false), "1. a\n   1. b|");
    assert_eq!(
        tab_over("1. a\n02. [b\n03. c]\n04. d", false),
        "1. a\n   1. [b\n   2. c]\n2. d"
    );
}

#[test]
fn in_a_quote_items_move_after_the_quote_marker() {
    assert_eq!(tab("> - a\n> - b|", false), "> - a\n>   - b|");
    assert_eq!(
        tab("> 1. a\n> 2. b|\n> 3. c", false),
        "> 1. a\n>    1. b|\n> 2. c"
    );
    assert_eq!(tab("> - a\n>   - b|", true), "> - a\n> - b|");
    assert_eq!(
        tab("> - a\n> - b|\n>   more 😀\n> - c", false),
        "> - a\n>   - b|\n>     more 😀\n> - c",
        "every line of the item"
    );
    // A quote inside an indented list item.
    assert_eq!(
        tab("- a\n    > - x\n    > - y|", false),
        "- a\n    > - x\n    >   - y|"
    );
    assert_eq!(
        tab("- x\n  - a\n    > - b\n    >   - c|", true),
        "- x\n  - a\n    > - b\n    > - c|"
    );
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
fn tab_and_shift_tab_follow_the_lists_own_indentation() {
    assert_eq!(tab("- a\n\t- b\n\t- c|", false), "- a\n\t- b\n\t\t- c|");
    assert_eq!(tab("- a\n\t- b|", true), "- a\n- b|");
    assert_eq!(tab("- a\n    - b|", true), "- a\n- b|", "a 4-space nesting");
}

#[test]
fn shift_tab_on_an_ordered_item_renumbers_both_lists() {
    assert_eq!(
        tab("1. a\n   1. b|\n   2. c\n2. d", true),
        "1. a\n2. b|\n   1. c\n3. d",
        "c stays under b as a list from 1; d moves up"
    );
    assert_eq!(
        tab("1. a\n2. b\n   1. x|\n3. c", true),
        "1. a\n2. b\n3. x|\n4. c"
    );
}

#[test]
fn shift_enter_in_fenced_code_in_a_list_item_keeps_the_indentation() {
    let mut doc = doc("- ```\n  code|\n  ```\n");
    soft_break(&mut doc, Duration::ZERO);
    assert_eq!(shown(&doc), "- ```\n  code\n  |\n  ```\n");
}

#[test]
fn tab_and_shift_tab_on_mixed_task_and_lone_cr_lists() {
    // Tab on a task item nests it by the marker, not the box.
    assert_eq!(tab("- [ ] a\n- [ ] b|", false), "- [ ] a\n  - [ ] b|");
    // A space-indented list in a document that also has a tab-indented one
    // goes under its sibling's children as they are indented.
    assert_eq!(
        tab("- x\n\t- y\n\n- a\n  - b\n- c|", false),
        "- x\n\t- y\n\n- a\n  - b\n  - c|"
    );
    assert_eq!(
        tab("- x\n\t- y\n\n- a\n  - b\n  - c|", true),
        "- x\n\t- y\n\n- a\n  - b\n- c|",
        "and back out, without underflow"
    );
    // Lone `\r` endings move every line of the item.
    assert_eq!(
        tab("- a\r\t- b|\r\t\t- c\r- d", true),
        "- a\r- b|\r\t- c\r- d"
    );
    // Following siblings stay under a lifted ordered item whose marker
    // grows (a bullet parent, then a number gaining a digit).
    assert_eq!(
        tab("- a\n  1. x|\n  2. y\n- b", true),
        "- a\n1. x|\n   1. y\n- b"
    );
    assert_eq!(
        tab("9. a\n   1. x|\n   2. y\n10. b", true),
        "9. a\n10. x|\n    1. y\n11. b"
    );
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
