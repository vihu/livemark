//! Editing commands over a [`Doc`]: caret motions that need no layout, and
//! typing, deleting, Enter and paste as transactions (REFERENCE-001
//! sections 13, 15 and 18). Motions that need layout (up, down, pages) are
//! the widget's.
use std::time::Duration;

use unicode_segmentation::UnicodeSegmentation;

use crate::doc::{Change, Doc, Kind, Selection};

/// A caret motion that needs no layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// One grapheme back; `\r\n` is one.
    Left,
    /// One grapheme on.
    Right,
    /// Back over spaces, then a run of word characters or of punctuation.
    WordLeft,
    /// On over spaces, then a run of word characters or of punctuation.
    WordRight,
    /// The start of the source line.
    LineStart,
    /// The end of the source line, before its ending.
    LineEnd,
    /// The start of the document.
    DocumentStart,
    /// The end of the document.
    DocumentEnd,
}

/// Where `motion` takes a caret at `from`.
pub fn target(doc: &Doc, from: usize, motion: Motion) -> usize {
    let text = doc.text();
    match motion {
        Motion::Left => text[..from]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i),
        Motion::Right => text[from..]
            .graphemes(true)
            .next()
            .map_or(from, |g| from + g.len()),
        Motion::WordLeft => word_left(text, from),
        Motion::WordRight => word_right(text, from),
        Motion::LineStart => doc.line_range(doc.line_at(from)).start,
        Motion::LineEnd => doc.line_range(doc.line_at(from)).end,
        Motion::DocumentStart => 0,
        Motion::DocumentEnd => text.len(),
    }
}

/// Moves the caret, or with `extend` only the selection's head. Left and
/// Right without `extend` collapse a selection to its edge instead
/// (CodeMirror's `cursorCharLeft`).
pub fn go(doc: &mut Doc, motion: Motion, extend: bool) {
    let selection = doc.selection();
    let range = selection.range();
    let head = match motion {
        Motion::Left if !extend && !range.is_empty() => range.start,
        Motion::Right if !extend && !range.is_empty() => range.end,
        _ => target(doc, selection.head, motion),
    };
    doc.set_selection(if extend {
        Selection {
            anchor: selection.anchor,
            head,
        }
    } else {
        Selection::caret(head)
    });
}

/// Replaces the selection with typed `text`.
pub fn type_text(doc: &mut Doc, text: &str, now: Duration) {
    replace_selection(doc, text, Kind::Type, now);
}

/// Enter outside lists, quotes and code: the document's line ending
/// (REFERENCE-001 section 13).
pub fn enter(doc: &mut Doc, now: Duration) {
    let ending = line_ending(doc.text());
    replace_selection(doc, ending, Kind::Other, now);
}

/// Replaces the selection with pasted `text`, its line breaks turned into
/// the document's line ending (REFERENCE-001 section 18).
pub fn paste(doc: &mut Doc, text: &str, now: Duration) {
    let ending = line_ending(doc.text());
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let text = if ending == "\n" {
        text
    } else {
        text.replace('\n', ending)
    };
    replace_selection(doc, &text, Kind::Other, now);
}

/// Deletes the selection, or from the caret to where `motion` takes it
/// (Backspace is [`Motion::Left`], Delete [`Motion::Right`]).
pub fn delete(doc: &mut Doc, motion: Motion, now: Duration) {
    let range = doc.selection().range();
    let range = if range.is_empty() {
        let to = target(doc, range.start, motion);
        to.min(range.start)..to.max(range.start)
    } else {
        range
    };
    if !range.is_empty() {
        let start = range.start;
        doc.apply(
            vec![Change::delete(range)],
            Selection::caret(start),
            Kind::Delete,
            now,
        );
    }
}

fn replace_selection(doc: &mut Doc, text: &str, kind: Kind, now: Duration) {
    let range = doc.selection().range();
    let caret = range.start + text.len();
    doc.apply(
        vec![Change {
            range,
            text: text.to_owned(),
        }],
        Selection::caret(caret),
        kind,
        now,
    );
}

/// The document's line ending: its first one, else `\n`, so edits never
/// mix endings.
pub fn line_ending(text: &str) -> &'static str {
    match text.find(['\n', '\r']).map(|i| &text.as_bytes()[i..]) {
        Some([b'\r', b'\n', ..]) => "\r\n",
        Some([b'\r', ..]) => "\r",
        _ => "\n",
    }
}

#[derive(PartialEq)]
enum Class {
    Space,
    Break,
    Word,
    Other,
}

fn class(c: char) -> Class {
    match c {
        '\n' | '\r' => Class::Break,
        c if c.is_whitespace() => Class::Space,
        c if c.is_alphanumeric() || c == '_' => Class::Word,
        _ => Class::Other,
    }
}

/// Skips spaces, then one run of a class; a line ending is a step alone
/// (REFERENCE-001 section 13).
fn word_right(text: &str, from: usize) -> usize {
    let mut chars = text[from..].char_indices().peekable();
    let mut end = from;
    if chars.peek().is_some_and(|&(_, c)| class(c) == Class::Break) {
        return text[from..]
            .graphemes(true)
            .next()
            .map_or(from, |g| from + g.len());
    }
    while let Some(&(i, c)) = chars.peek() {
        if class(c) != Class::Space {
            break;
        }
        end = from + i + c.len_utf8();
        chars.next();
    }
    let Some(&(_, first)) = chars.peek() else {
        return end;
    };
    let run = class(first);
    if run == Class::Break {
        return end;
    }
    for (i, c) in chars {
        if class(c) != run {
            break;
        }
        end = from + i + c.len_utf8();
    }
    end
}

fn word_left(text: &str, from: usize) -> usize {
    let mut chars = text[..from].char_indices().rev().peekable();
    let mut start = from;
    if chars.peek().is_some_and(|&(_, c)| class(c) == Class::Break) {
        return text[..from]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i);
    }
    while let Some(&(i, c)) = chars.peek() {
        if class(c) != Class::Space {
            break;
        }
        start = i;
        chars.next();
    }
    let Some(&(_, first)) = chars.peek() else {
        return start;
    };
    let run = class(first);
    if run == Class::Break {
        return start;
    }
    for (i, c) in chars {
        if class(c) != run {
            break;
        }
        start = i;
    }
    start
}

#[cfg(test)]
mod tests;
