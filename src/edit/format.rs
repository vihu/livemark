//! Formatting keys (REFERENCE-001 section 12): bold, italic and inline code
//! toggle on the selection, the word around the caret, or a new pair of
//! markers; Ctrl/Cmd+K makes a link. Each is one undo step. Whether the
//! caret is inside bold, italic or code comes from the parsed constructs,
//! not from the characters around it: an italic toggle inside `**bold**`
//! must not take one `*` from each side.
use std::time::Duration;

use super::word_at;
use crate::doc::{Change, Doc, Kind, Selection};
use crate::style::{Styled, Syntax};

/// An inline format with a toggle key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// `**bold**`, Ctrl/Cmd+B.
    Bold,
    /// `*italic*`, Ctrl/Cmd+I.
    Italic,
    /// `` `code` ``, Ctrl/Cmd+E (the user's pick, 2026-10-02).
    Code,
}

impl Format {
    fn syntax(self) -> Syntax {
        match self {
            Format::Bold => Syntax::Strong,
            Format::Italic => Syntax::Emphasis,
            Format::Code => Syntax::Code,
        }
    }
}

/// Toggles `format` (REFERENCE-001 section 12): off when the selection or
/// caret is inside a construct of that kind; else on the selection, on the
/// word the caret is inside, or as an empty pair around the caret (or a
/// step over the closing marker the caret is right before).
pub fn toggle(doc: &mut Doc, styled: &Styled, format: Format, now: Duration) {
    let selection = doc.selection();
    let range = selection.range();
    let syntax = format.syntax();
    let inside = styled
        .constructs()
        .iter()
        .filter(|c| c.syntax == syntax && c.range.start <= range.start && range.end <= c.range.end)
        .min_by_key(|c| c.range.len());
    if let Some(construct) = inside {
        let [open, close] = construct.markers.clone();
        if range.is_empty() && range.start == close.start && range.start > open.end {
            // Right before the closing marker: step over it.
            doc.set_selection(Selection::caret(close.end));
            return;
        }
        let map = |at: usize| {
            if at <= open.start {
                at
            } else if at <= open.end {
                open.start
            } else if at <= close.start {
                at - open.len()
            } else {
                close.start - open.len()
            }
        };
        let selection = Selection {
            anchor: map(selection.anchor),
            head: map(selection.head),
        };
        doc.apply(
            vec![Change::delete(open), Change::delete(close)],
            selection,
            Kind::Other,
            now,
        );
        return;
    }
    let text = doc.text();
    let target = if range.is_empty() {
        strictly_in_word(doc, range.start).unwrap_or(range.clone())
    } else {
        range.clone()
    };
    let marker = match format {
        Format::Bold => "**".to_owned(),
        Format::Italic => "*".to_owned(),
        // A code span's fence is longer than any backtick run inside it.
        Format::Code => {
            let longest = text[target.clone()]
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or(0);
            "`".repeat(longest + 1)
        }
    };
    let width = marker.len();
    let moved = |at: usize| {
        if at < target.start || (at == target.start && !range.is_empty()) {
            at + if at == target.start { width } else { 0 }
        } else if at <= target.end {
            at + width
        } else {
            at + 2 * width
        }
    };
    let selection = if range.is_empty() {
        Selection::caret(moved(range.start))
    } else {
        Selection {
            anchor: moved(selection.anchor),
            head: moved(selection.head),
        }
    };
    doc.apply(
        vec![
            Change::insert(target.start, marker.clone()),
            Change::insert(target.end, marker),
        ],
        selection,
        Kind::Other,
        now,
    );
}

/// The word the caret at `at` is strictly inside: word characters on both
/// sides (at a word's edge an empty pair is typed instead).
fn strictly_in_word(doc: &Doc, at: usize) -> Option<std::ops::Range<usize>> {
    let text = doc.text();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let before = text[..at].chars().next_back().is_some_and(word);
    let after = text[at..].chars().next().is_some_and(word);
    (before && after).then(|| word_at(doc, at, false))
}

/// Ctrl/Cmd+K (REFERENCE-001 section 12): a selected URL becomes
/// `[](url)` with the caret in the brackets; other selected text becomes
/// `[text]()` with the caret in the parentheses; nothing selected gives
/// `[]()` with the caret in the brackets.
pub fn link(doc: &mut Doc, now: Duration) {
    let range = doc.selection().range();
    let selected = &doc.text()[range.clone()];
    let (insert, caret) = if selected.is_empty() {
        ("[]()".to_owned(), 1)
    } else if is_url(selected) {
        (format!("[]({selected})"), 1)
    } else {
        let insert = format!("[{selected}]()");
        let caret = insert.len() - 1;
        (insert, caret)
    };
    let caret = range.start + caret;
    doc.apply(
        vec![Change {
            range,
            text: insert,
        }],
        Selection::caret(caret),
        Kind::Other,
        now,
    );
}

/// An absolute URL: a scheme, `:`, and no whitespace (what SilverBullet's
/// `new URL` check accepts, near enough).
fn is_url(text: &str) -> bool {
    let Some((scheme, rest)) = text.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && !rest.is_empty()
        && !text.contains(char::is_whitespace)
}

#[cfg(test)]
mod tests;
