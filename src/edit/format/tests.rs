//! Formatting keys per REFERENCE-001 section 12. `[` and `]` mark the
//! selection (anchor first), `|` a caret.
use std::time::Duration;

use super::{Format, link, toggle};
use crate::doc::{Doc, Selection};
use crate::style::Styled;

fn doc(marked: &str) -> Doc {
    let (text, selection) = if let Some(caret) = marked.find('|') {
        (marked.replacen('|', "", 1), Selection::caret(caret))
    } else {
        let anchor = marked.find('[').expect("a selection");
        let head = marked.find(']').expect("its end") - 1;
        (
            marked.replacen('[', "", 1).replacen(']', "", 1),
            Selection { anchor, head },
        )
    };
    let mut doc = Doc::new(text);
    doc.set_selection(selection);
    doc
}

fn shown(doc: &Doc) -> String {
    let mut text = doc.text().to_owned();
    let range = doc.selection().range();
    if range.is_empty() {
        text.insert(range.start, '|');
    } else {
        text.insert(range.end, ']');
        text.insert(range.start, '[');
    }
    text
}

fn apply(marked: &str, format: Format) -> String {
    let mut doc = doc(marked);
    let styled = Styled::new(doc.text());
    toggle(&mut doc, &styled, format, Duration::ZERO);
    shown(&doc)
}

#[test]
fn a_caret_inside_a_word_formats_the_word() {
    assert_eq!(
        apply("say hel|lo there", Format::Bold),
        "say **hel|lo** there"
    );
    assert_eq!(
        apply("say hel|lo there", Format::Italic),
        "say *hel|lo* there"
    );
}

#[test]
fn a_caret_elsewhere_types_a_pair_or_steps_over_the_closing_marker() {
    assert_eq!(apply("hello |", Format::Bold), "hello **|**");
    assert_eq!(
        apply("hello| there", Format::Code),
        "hello`|` there",
        "at a word's edge"
    );
    assert_eq!(apply("**bold|**", Format::Bold), "**bold**|");
}

#[test]
fn inside_the_format_toggles_it_off() {
    assert_eq!(apply("**bo|ld**", Format::Bold), "bo|ld");
    assert_eq!(
        apply("**[bold]**", Format::Bold),
        "[bold]",
        "the selection kept"
    );
    assert_eq!(apply("`co|de`", Format::Code), "co|de");
    assert_eq!(
        apply("***bo|th***", Format::Bold),
        "*bo|th*",
        "the strong inside emphasis"
    );
}

#[test]
fn a_selection_is_wrapped_and_stays_selected() {
    assert_eq!(apply("a [bc] d", Format::Bold), "a **[bc]** d");
    assert_eq!(
        apply("**b[c]d**", Format::Italic),
        "**b*[c]*d**",
        "italic in bold"
    );
    assert_eq!(
        apply("x [a`b] y", Format::Code),
        "x ``[a`b]`` y",
        "a longer fence"
    );
}

#[test]
fn ctrl_k_makes_a_link_from_the_selection() {
    let link_of = |marked: &str| {
        let mut doc = doc(marked);
        link(&mut doc, Duration::ZERO);
        shown(&doc)
    };
    assert_eq!(link_of("[https://example.com]"), "[|](https://example.com)");
    assert_eq!(link_of("[some text]"), "[some text](|)");
    assert_eq!(link_of("|"), "[|]()");
}

#[test]
fn in_a_table_cell_the_format_toggles_off_too() {
    // Built by hand: the helpers read a table's pipes as the caret.
    for (text, at, format, after) in [
        (
            "| **ab** | c |\n| - | - |\n",
            5,
            Format::Bold,
            "| ab | c |\n| - | - |\n",
        ),
        (
            "| x | `cd` |\n| - | - |\n",
            8,
            Format::Code,
            "| x | cd |\n| - | - |\n",
        ),
    ] {
        let mut doc = Doc::new(text.into());
        doc.set_selection(Selection::caret(at));
        let styled = Styled::new(doc.text());
        toggle(&mut doc, &styled, format, Duration::ZERO);
        assert_eq!(doc.text(), after);
    }
}

#[test]
fn a_selection_is_wrapped_line_by_line_without_markup_or_spaces() {
    // A triple-clicked list line: the item stays, the stars hug its text.
    assert_eq!(
        apply("[- Book the room\n]next", Format::Bold),
        "[- **Book the room**\n]next"
    );
    // A drag ending in a space.
    assert_eq!(
        apply("We meet [Thursday. ]We", Format::Bold),
        "We meet **[Thursday.** ]We"
    );
    // Two items, a heading in a quote.
    assert_eq!(apply("[- a\n- b]", Format::Italic), "[- *a*\n- *b]*");
    assert_eq!(apply("[> # Title]", Format::Bold), "[> # **Title]**");
    // A line already in code stays as it is; the other is wrapped.
    assert_eq!(apply("[1. `a`\n2. b]", Format::Code), "[1. `a`\n2. `b]`");
}

#[test]
fn a_second_press_over_several_lines_takes_the_format_off() {
    for format in [Format::Bold, Format::Italic, Format::Code] {
        for text in ["first line\nsecond line\n", "- a\n- b", "> one\n> two"] {
            let mut doc = Doc::new(text.into());
            doc.set_selection(Selection {
                anchor: 0,
                head: text.len(),
            });
            for _ in 0..2 {
                let styled = Styled::new(doc.text());
                toggle(&mut doc, &styled, format, Duration::ZERO);
            }
            assert_eq!(doc.text(), text, "{format:?}");
        }
    }
}

#[test]
fn a_hard_break_backslash_stays_outside_the_markers() {
    let mut doc = Doc::new("line \\\nnext".into());
    doc.set_selection(Selection {
        anchor: 0,
        head: 11,
    });
    let styled = Styled::new(doc.text());
    toggle(&mut doc, &styled, Format::Bold, Duration::ZERO);
    assert_eq!(doc.text(), "**line** \\\n**next**");
}

/// `format` pressed twice over `anchor..head` of `text`: the text after
/// each press.
fn twice(text: &str, (anchor, head): (usize, usize), format: Format) -> [String; 2] {
    let mut doc = Doc::new(text.into());
    doc.set_selection(Selection { anchor, head });
    [0, 1].map(|_| {
        let styled = Styled::new(doc.text());
        toggle(&mut doc, &styled, format, Duration::ZERO);
        doc.text().to_owned()
    })
}

#[test]
fn twice_over_part_of_several_lines_gives_the_text_back() {
    let text = "- buy milk\n- call bob";
    for format in [Format::Bold, Format::Italic] {
        assert_eq!(twice(text, (6, 17), format)[1], text, "{format:?}");
    }
    // From a line's end: that line is not touched.
    assert_eq!(
        twice("a\nb\nc", (1, 3), Format::Bold),
        ["a\n**b**\nc", "a\nb\nc"]
    );
    assert_eq!(twice("**a**\n**b**", (5, 11), Format::Bold)[0], "**a**\nb");
    // Bold over italic lines and back.
    assert_eq!(twice("*a*\n*b*", (0, 7), Format::Bold)[1], "*a*\n*b*");
}

#[test]
fn over_several_lines_only_inline_text_is_wrapped() {
    let text = "para\n```\ncode\n```\n| a | b |\n| - | - |\n| c | d |\n---\nend";
    let [once, _] = twice(text, (0, text.len()), Format::Bold);
    assert_eq!(
        once,
        "**para**\n```\ncode\n```\n| a | b |\n| - | - |\n| c | d |\n---\n**end**"
    );
    let setext = "Title\n=====\nnext";
    assert_eq!(
        twice(setext, (0, setext.len()), Format::Bold)[0],
        "**Title**\n=====\n**next**"
    );
    // Nothing but spaces or markup: nothing to wrap.
    assert_eq!(twice("   \n  \n", (0, 6), Format::Bold)[0], "   \n  \n");
    assert_eq!(twice("- \n> \n", (0, 6), Format::Bold)[0], "- \n> \n");
}

#[test]
fn a_line_of_markers_alone_and_a_backslash_in_code_come_back() {
    let text = "_\nb\n";
    assert_eq!(twice(text, (0, 4), Format::Bold)[1], text);
    // In code a backslash is text, not a hard break.
    assert_eq!(
        twice("end C:\\dir\\\nnext", (4, 16), Format::Code)[0],
        "end `C:\\dir\\`\n`next`"
    );
}
