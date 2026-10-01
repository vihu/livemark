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
