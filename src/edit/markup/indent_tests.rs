//! Tab and Shift+Tab over list items and the lines a selection reaches,
//! per REFERENCE-001 section 7. `|` marks the caret in each case.
use std::time::Duration;

use super::indent;
use super::tests::{doc, shown};
use crate::doc::{Doc, Selection};

pub(super) fn tab(text: &str, outdent: bool) -> String {
    let mut doc = doc(text);
    indent(&mut doc, outdent, Duration::ZERO);
    shown(&doc)
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
fn shift_tab_leaves_a_list_with_no_item_around_it() {
    for text in [
        " - a|\n - b",
        " 8. eight|\n 9. nine\n10. ten",
        ">  - a|\n>  - b",
    ] {
        assert_eq!(tab(text, true), text);
    }
}

#[test]
fn tab_and_shift_tab_land_on_the_items_text_column() {
    // A tab short of the text's column becomes spaces.
    assert_eq!(
        tab("- t\n\t- u\n\n- p\n  1. a\n  2. b|", false),
        "- t\n\t- u\n\n- p\n  1. a\n     1. b|"
    );
    assert_eq!(
        tab("- t\n\n> - a\n> \t1. x|\n> \t2. y", true),
        "- t\n\n> - a\n> 1. x|\n>    1. y"
    );
    // Children move by columns, not by the text in front of a tab; an item
    // joining an ordered child list continues its numbers.
    assert_eq!(
        tab("1. a\n   1. x\n2. b|\n\t- c", false),
        "1. a\n   1. x\n   2. b|\n       - c"
    );
    // A `>` with no space after it takes the first space as its own.
    assert_eq!(tab(">- a\n>- b|", false), ">- a\n>   - b|");
    assert_eq!(tab(">1. a\n>2. b|", false), ">1. a\n>    1. b|");
}

#[test]
fn children_move_with_a_number_changing_width() {
    // The lifted item's own, an outer sibling's, a following sibling's.
    assert_eq!(
        tab("9. a\n   1. x|\n      - c", true),
        "9. a\n10. x|\n    - c"
    );
    assert_eq!(
        tab("8. a\n   1. b|\n9. c\n   - d", true),
        "8. a\n9. b|\n10. c\n    - d"
    );
    assert_eq!(
        tab("8. a\n9. b|\n10. c\n    - d", false),
        "8. a\n   1. b|\n9. c\n   - d"
    );
}

#[test]
fn shift_tab_into_another_kind_of_list_keeps_both_numbers() {
    // Another delimiter: a list of its own, the outer one left as it is.
    assert_eq!(tab("7. a\n   1) b|\n8. c", true), "7. a\n1) b|\n8. c");
    // Siblings left under the lifted item join its ordered child list.
    assert_eq!(
        tab("1. a\n   1. b|\n      1. x\n   2. c", true),
        "1. a\n2. b|\n   1. x\n   2. c"
    );
}

#[test]
fn shift_tab_in_fenced_code_keeps_the_items_indentation() {
    assert_eq!(
        tab("- a\n  ```\n  x|\n  ```", true),
        "- a\n  ```\n  x|\n  ```"
    );
    assert_eq!(
        tab("- a\n  ```\n      x|\n  ```", true),
        "- a\n  ```\n  x|\n  ```"
    );
}

#[test]
fn tab_and_shift_tab_measure_items_after_a_parents_marker() {
    assert_eq!(tab("1. - a\n   - b|", false), "1. - a\n     - b|");
    assert_eq!(tab("1. - a\n     - b|", true), "1. - a\n   - b|");
    assert_eq!(tab("- > 1. a\n  > 2. b|", false), "- > 1. a\n  >    1. b|");
    assert_eq!(
        tab("- - a|", true),
        "- - a|",
        "an item on its parent's line stays"
    );
}
